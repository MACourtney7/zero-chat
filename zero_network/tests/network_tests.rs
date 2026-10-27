//! Comprehensive integration tests for `zero_network` signaling, WebSocket hibernation attachment,
//! constant-time HMAC-SHA256 verification, single-use invite burning, and dead-drop piping.

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use zero_network::{
    default_ice_servers_json, extract_bearer_token, parse_custom_ice_servers, parse_host_secret,
    process_dead_drop, process_host_upgrade, process_ice_servers, EphemeralRoomState,
    HibernatingSocket, HostAttachment, MockHostSocket, NetworkError, MAX_BURNED_UUIDS,
    MAX_DROP_BODY_BYTES,
};
use zero_protocol::crypto::{compute_invite_token, X25519Keypair};
use zero_protocol::{from_json_str, to_json_bytes, DropPayload, SdpAnswer};

fn sample_drop_payload(invite_uuid: &str, token: &str) -> DropPayload {
    let joiner_kp = X25519Keypair::from_private_key([0x42; 32]);
    DropPayload {
        invite_uuid: invite_uuid.to_string(),
        token: token.to_string(),
        sdp_answer: SdpAnswer {
            sdp: "v=0\r\no=- 999 2 IN IP4 127.0.0.1\r\na=setup:active\r\n".to_string(),
        },
        joiner_public_key: *joiner_kp.public_key_bytes(),
        joiner_peer_id: None,
        joiner_signing_key: None,
        sealed_sdp_answer: None,
        signature: None,
    }
}

#[test]
fn test_host_upgrade_accepts_hex_and_base64_secrets() {
    let raw_secret = [0xABu8; 32];
    let hex_secret = hex::encode(raw_secret);
    let b64_std_secret = STANDARD.encode(raw_secret);
    let b64_url_secret = URL_SAFE_NO_PAD.encode(raw_secret);

    // 1. 64-char hex secret
    let mut socket_hex = MockHostSocket::new();
    let res_hex = process_host_upgrade(Some("websocket"), Some(&hex_secret), &mut socket_hex)
        .expect("hex secret upgrade should succeed");
    assert_eq!(res_hex.status, 101);
    assert!(socket_hex.hibernating);
    assert_eq!(
        socket_hex.recover_host_secret().unwrap(),
        Some(HostAttachment {
            host_secret: raw_secret
        })
    );

    // 2. Standard Base64 (44 chars) with Bearer prefix
    let mut socket_b64 = MockHostSocket::new();
    let bearer_b64 = format!("Bearer {b64_std_secret}");
    let res_b64 = process_host_upgrade(Some("WebSocket"), Some(&bearer_b64), &mut socket_b64)
        .expect("base64 secret upgrade should succeed");
    assert_eq!(res_b64.status, 101);
    assert!(socket_b64.hibernating);
    assert_eq!(
        socket_b64.recover_host_secret().unwrap(),
        Some(HostAttachment {
            host_secret: raw_secret
        })
    );

    // 3. Base64URL no-pad (43 chars)
    let mut socket_url = MockHostSocket::new();
    let res_url = process_host_upgrade(Some("websocket"), Some(&b64_url_secret), &mut socket_url)
        .expect("base64url secret upgrade should succeed");
    assert_eq!(res_url.status, 101);
    assert_eq!(
        socket_url.recover_host_secret().unwrap(),
        Some(HostAttachment {
            host_secret: raw_secret
        })
    );

    // 4. Missing or invalid Upgrade header -> 426 Upgrade Required
    let mut socket_no_upg = MockHostSocket::new();
    let err_no_upg = process_host_upgrade(None, Some(&hex_secret), &mut socket_no_upg)
        .expect_err("missing upgrade header must fail");
    assert_eq!(err_no_upg, NetworkError::MissingUpgradeHeader);
    assert_eq!(err_no_upg.status_code(), 426);
    assert!(!socket_no_upg.hibernating);

    // 5. Malformed or wrong-length secret -> 400 Bad Request
    let mut socket_bad_sec = MockHostSocket::new();
    let err_short = process_host_upgrade(Some("websocket"), Some("deadbeef"), &mut socket_bad_sec)
        .expect_err("short secret must fail");
    assert_eq!(err_short, NetworkError::InvalidHostSecret);
    assert_eq!(err_short.status_code(), 400);

    let err_missing = process_host_upgrade(Some("websocket"), None, &mut socket_bad_sec)
        .expect_err("missing secret must fail");
    assert_eq!(err_missing, NetworkError::InvalidHostSecret);
    assert_eq!(err_missing.status_code(), 400);

    // Verify extract_bearer_token helper directly
    assert_eq!(
        extract_bearer_token(Some("  Bearer abc123  ")),
        Some("abc123")
    );
    assert_eq!(extract_bearer_token(Some("bearer   xyz")), Some("xyz"));
    assert_eq!(extract_bearer_token(Some("Bearer   ")), None);
    assert_eq!(extract_bearer_token(Some("   ")), None);
    assert_eq!(extract_bearer_token(None), None);
    assert_eq!(parse_host_secret(&hex_secret).unwrap(), raw_secret);
}

#[test]
fn test_dead_drop_rejects_invalid_or_tampered_hmac_with_403() {
    let host_secret = [0x55u8; 32];
    let invite_uuid = "invite-uuid-001";

    let mut host_socket = MockHostSocket::new();
    process_host_upgrade(
        Some("websocket"),
        Some(&hex::encode(host_secret)),
        &mut host_socket,
    )
    .expect("host upgrade");

    let mut sockets = vec![host_socket];
    let mut room_state = EphemeralRoomState::new();

    // Compute valid token and tamper with the last character
    let valid_token = compute_invite_token(&host_secret, invite_uuid)
        .expect("compute token")
        .hmac_hex;
    let mut tampered_token = valid_token.clone();
    let last_char = tampered_token.pop().unwrap();
    tampered_token.push(if last_char == 'a' { 'b' } else { 'a' });

    let bad_payload = sample_drop_payload(invite_uuid, &tampered_token);
    let bad_bytes = to_json_bytes(&bad_payload).expect("serialize payload");

    let err = process_dead_drop(invite_uuid, None, &bad_bytes, &mut room_state, &mut sockets)
        .expect_err("tampered HMAC must be rejected");
    assert_eq!(err, NetworkError::ForbiddenHmac);
    assert_eq!(err.status_code(), 403);

    // Verify invite UUID was NOT burned and no message was sent to the host WebSocket
    assert!(!room_state.is_burned(invite_uuid));
    assert_eq!(room_state.burned_count(), 0);
    assert!(sockets[0].sent_messages.is_empty());

    // Also test token signed with wrong host_secret
    let wrong_secret = [0x99u8; 32];
    let wrong_key_token = compute_invite_token(&wrong_secret, invite_uuid)
        .expect("compute wrong key token")
        .hmac_hex;
    let wrong_payload = sample_drop_payload(invite_uuid, &wrong_key_token);
    let wrong_bytes = to_json_bytes(&wrong_payload).expect("serialize wrong payload");

    let err_wrong_key = process_dead_drop(
        invite_uuid,
        Some(&format!("Bearer {wrong_key_token}")),
        &wrong_bytes,
        &mut room_state,
        &mut sockets,
    )
    .expect_err("wrong key HMAC must be rejected");
    assert_eq!(err_wrong_key, NetworkError::ForbiddenHmac);
    assert_eq!(err_wrong_key.status_code(), 403);
    assert!(!room_state.is_burned(invite_uuid));
    assert!(sockets[0].sent_messages.is_empty());

    // Also test empty token
    let empty_token_payload = sample_drop_payload(invite_uuid, "   ");
    let empty_token_bytes = to_json_bytes(&empty_token_payload).expect("serialize");
    let err_empty = process_dead_drop(
        invite_uuid,
        None,
        &empty_token_bytes,
        &mut room_state,
        &mut sockets,
    )
    .expect_err("empty token must be rejected");
    assert_eq!(err_empty, NetworkError::ForbiddenHmac);
    assert_eq!(err_empty.status_code(), 403);
}

#[test]
fn test_dead_drop_burns_uuid_and_rejects_replay_with_410() {
    let host_secret = [0x77u8; 32];
    let invite_uuid = "invite-single-use-42";

    let mut host_socket = MockHostSocket::new();
    process_host_upgrade(
        Some("websocket"),
        Some(&hex::encode(host_secret)),
        &mut host_socket,
    )
    .expect("host upgrade");

    let mut sockets = vec![host_socket];
    let mut room_state = EphemeralRoomState::new();

    let token = compute_invite_token(&host_secret, invite_uuid)
        .expect("compute token")
        .hmac_hex;
    let payload = sample_drop_payload(invite_uuid, &token);
    let body_bytes = to_json_bytes(&payload).expect("serialize payload");

    // First submission succeeds with HTTP 200 and burns the invite_uuid
    let first_res = process_dead_drop(
        invite_uuid,
        Some(&format!("Bearer {token}")),
        &body_bytes,
        &mut room_state,
        &mut sockets,
    )
    .expect("first dead drop should succeed");
    assert_eq!(first_res.status, 200);
    assert!(room_state.is_burned(invite_uuid));
    assert_eq!(room_state.burned_count(), 1);
    assert_eq!(sockets[0].sent_messages.len(), 1);

    // Second submission (replay) with identical valid token is rejected with HTTP 410 Gone
    let replay_err = process_dead_drop(
        invite_uuid,
        Some(&format!("Bearer {token}")),
        &body_bytes,
        &mut room_state,
        &mut sockets,
    )
    .expect_err("replayed invite_uuid must fail with 410 Gone");
    assert_eq!(replay_err, NetworkError::InviteAlreadyBurned);
    assert_eq!(replay_err.status_code(), 410);

    // Ensure no second message was piped to the host WebSocket
    assert_eq!(sockets[0].sent_messages.len(), 1);
}

#[test]
fn test_valid_dead_drop_pipes_answer_to_hibernating_websocket() {
    let host_secret = [0x33u8; 32];
    let invite_uuid = "invite-e2e-handshake-001";

    // Step 1: Host upgrades WebSocket and hibernates with attached secret
    let mut host_socket = MockHostSocket::new();
    let upgrade_res = process_host_upgrade(
        Some("websocket"),
        Some(&URL_SAFE_NO_PAD.encode(host_secret)),
        &mut host_socket,
    )
    .expect("host upgrade should succeed");
    assert_eq!(upgrade_res.status, 101);
    assert!(host_socket.hibernating);

    // Step 2: Host generates HMAC-SHA256 invite token
    let invite_token = compute_invite_token(&host_secret, invite_uuid).expect("compute token");

    // Step 3: Joiner posts DropPayload containing SDP answer and X25519 public key
    let expected_payload = sample_drop_payload(invite_uuid, &invite_token.hmac_hex);
    let body_bytes = to_json_bytes(&expected_payload).expect("serialize DropPayload");

    let mut sockets = vec![host_socket];
    let mut room_state = EphemeralRoomState::new();

    let drop_res = process_dead_drop(
        invite_uuid,
        None,
        &body_bytes,
        &mut room_state,
        &mut sockets,
    )
    .expect("valid dead drop should succeed");
    assert_eq!(drop_res.status, 200);
    assert_eq!(drop_res.body, r#"{"status":"ok"}"#);

    // Step 4: Host receives the exact DropPayload over the awakened WebSocket
    assert_eq!(sockets[0].sent_messages.len(), 1);
    let forwarded_json = &sockets[0].sent_messages[0];
    let received_payload: DropPayload =
        from_json_str(forwarded_json).expect("deserialize forwarded DropPayload");
    assert_eq!(received_payload, expected_payload);
}

#[test]
fn test_dead_drop_preserves_joiner_peer_id_to_host_websocket() {
    let host_secret = [0x44u8; 32];
    let invite_uuid = "invite-joiner-peer-id-001";

    let mut host_socket = MockHostSocket::new();
    process_host_upgrade(
        Some("websocket"),
        Some(&hex::encode(host_secret)),
        &mut host_socket,
    )
    .expect("host upgrade should succeed");

    let invite_token = compute_invite_token(&host_secret, invite_uuid).expect("compute token");

    let joiner_kp = X25519Keypair::from_private_key([0x55; 32]);
    let payload = DropPayload {
        invite_uuid: invite_uuid.to_string(),
        token: invite_token.hmac_hex.clone(),
        sdp_answer: SdpAnswer {
            sdp: "v=0\r\no=- 100 2 IN IP4 127.0.0.1\r\na=setup:active\r\n".to_string(),
        },
        joiner_public_key: *joiner_kp.public_key_bytes(),
        joiner_peer_id: Some("peer-bob-assigned-123".to_string()),
        joiner_signing_key: None,
        sealed_sdp_answer: None,
        signature: None,
    };
    let body_bytes = to_json_bytes(&payload).expect("serialize DropPayload");

    let mut sockets = vec![host_socket];
    let mut room_state = EphemeralRoomState::new();

    let drop_res = process_dead_drop(
        invite_uuid,
        None,
        &body_bytes,
        &mut room_state,
        &mut sockets,
    )
    .expect("dead drop with joiner_peer_id should succeed");
    assert_eq!(drop_res.status, 200);

    // Host receives the exact DropPayload containing joiner_peer_id
    assert_eq!(sockets[0].sent_messages.len(), 1);
    let forwarded_json = &sockets[0].sent_messages[0];
    assert!(forwarded_json.contains("\"joiner_peer_id\":\"peer-bob-assigned-123\""));

    let received_payload: DropPayload =
        from_json_str(forwarded_json).expect("deserialize forwarded DropPayload");
    assert_eq!(received_payload, payload);
    assert_eq!(
        received_payload.joiner_peer_id.as_deref(),
        Some("peer-bob-assigned-123")
    );
}

#[test]
fn test_dead_drop_edge_cases_and_dos_guards() {
    let host_secret = [0xAAu8; 32];
    let invite_uuid = "invite-edge-case";

    let mut host_socket = MockHostSocket::new();
    process_host_upgrade(
        Some("websocket"),
        Some(&hex::encode(host_secret)),
        &mut host_socket,
    )
    .expect("host upgrade");
    let mut sockets = vec![host_socket];
    let mut room_state = EphemeralRoomState::new();

    let valid_token = compute_invite_token(&host_secret, invite_uuid)
        .expect("token")
        .hmac_hex;
    let valid_payload = sample_drop_payload(invite_uuid, &valid_token);
    let valid_bytes = to_json_bytes(&valid_payload).expect("bytes");

    // 1. Mismatched path_invite_uuid vs payload.invite_uuid -> 400 Bad Request
    let err_mismatch = process_dead_drop(
        "different-invite-uuid",
        None,
        &valid_bytes,
        &mut room_state,
        &mut sockets,
    )
    .expect_err("mismatched invite_uuid must fail");
    assert_eq!(err_mismatch, NetworkError::InviteUuidMismatch);
    assert_eq!(err_mismatch.status_code(), 400);

    // 2. Oversized payload (> MAX_DROP_BODY_BYTES) -> 413 Payload Too Large
    let oversized = vec![b'a'; MAX_DROP_BODY_BYTES + 1];
    let err_large = process_dead_drop(invite_uuid, None, &oversized, &mut room_state, &mut sockets)
        .expect_err("oversized body must fail");
    assert_eq!(err_large, NetworkError::PayloadTooLarge);
    assert_eq!(err_large.status_code(), 413);

    // 3. Mismatched Authorization: Bearer header vs payload.token -> 403 Forbidden
    let err_header_mismatch = process_dead_drop(
        invite_uuid,
        Some("Bearer conflicting-token"),
        &valid_bytes,
        &mut room_state,
        &mut sockets,
    )
    .expect_err("conflicting header token must fail");
    assert_eq!(err_header_mismatch, NetworkError::ForbiddenHmac);
    assert_eq!(err_header_mismatch.status_code(), 403);

    // 4. Missing host socket -> 404 Not Found
    let mut empty_sockets: Vec<MockHostSocket> = Vec::new();
    let err_no_host = process_dead_drop(
        invite_uuid,
        None,
        &valid_bytes,
        &mut room_state,
        &mut empty_sockets,
    )
    .expect_err("missing host socket must return 404");
    assert_eq!(err_no_host, NetworkError::HostSocketUnavailable);
    assert_eq!(err_no_host.status_code(), 404);

    // 5. Burn capacity exhaustion -> 429 Too Many Requests
    let mut full_state = EphemeralRoomState::new();
    for i in 0..MAX_BURNED_UUIDS {
        assert!(full_state.burn_uuid(format!("burned-{i}")));
    }
    let err_capacity = process_dead_drop(
        invite_uuid,
        None,
        &valid_bytes,
        &mut full_state,
        &mut sockets,
    )
    .expect_err("full burn table must return 429");
    assert_eq!(err_capacity, NetworkError::BurnCapacityExceeded);
    assert_eq!(err_capacity.status_code(), 429);
}

#[test]
fn test_ice_endpoint_returns_default_stun_fallback() {
    let resp = process_ice_servers(None);
    assert_eq!(resp.status, 200);

    let json: serde_json::Value = serde_json::from_str(&resp.body).expect("parse JSON body");

    assert!(json.get("iceServers").is_some());
    let ice_servers = json["iceServers"]
        .as_array()
        .expect("iceServers should be an array");
    assert_eq!(ice_servers.len(), 2);

    let urls: Vec<&str> = ice_servers
        .iter()
        .map(|s| s["urls"].as_str().expect("urls string"))
        .collect();
    assert_eq!(
        urls,
        vec![
            "stun:stun.cloudflare.com:3478",
            "stun:stun.l.google.com:19302"
        ]
    );

    // Verify default_ice_servers_json helper consistency
    assert_eq!(json, default_ice_servers_json());
}

#[test]
fn test_parse_custom_ice_servers_modes() {
    // Mode 2a: Valid array wrapped into { "iceServers": ... }
    let raw_arr = r#"[{"urls":"turn:turn.example.com:3478","username":"user","credential":"pwd"}]"#;
    let parsed_arr = parse_custom_ice_servers(raw_arr).expect("parse custom array");
    let arr = parsed_arr["iceServers"]
        .as_array()
        .expect("iceServers array");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["urls"], "turn:turn.example.com:3478");

    // Mode 2b: Valid object already containing iceServers
    let raw_obj = r#"{"iceServers":[{"urls":"turn:turn.override.com:3478"}]}"#;
    let parsed_obj = parse_custom_ice_servers(raw_obj).expect("parse custom object");
    let arr2 = parsed_obj["iceServers"]
        .as_array()
        .expect("iceServers array");
    assert_eq!(arr2.len(), 1);
    assert_eq!(arr2[0]["urls"], "turn:turn.override.com:3478");

    // Mode 2c: Flat credential object containing urls
    let raw_flat = r#"{"urls":"turn:turn.cloudflare.com:3478","username":"u","credential":"p"}"#;
    let parsed_flat = parse_custom_ice_servers(raw_flat).expect("parse flat credential object");
    let arr3 = parsed_flat["iceServers"]
        .as_array()
        .expect("iceServers array");
    assert_eq!(arr3.len(), 1);
    assert_eq!(arr3[0]["urls"], "turn:turn.cloudflare.com:3478");
    assert_eq!(arr3[0]["username"], "u");
    assert_eq!(arr3[0]["credential"], "p");

    // Malformed JSON -> None
    assert!(parse_custom_ice_servers("not valid json").is_none());

    // Object without iceServers -> None
    assert!(parse_custom_ice_servers(r#"{"other":"key"}"#).is_none());

    // Primitive number -> None
    assert!(parse_custom_ice_servers("123").is_none());
}
