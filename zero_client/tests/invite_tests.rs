//! Comprehensive integration tests verifying REQ-004.1 through REQ-004.5:
//! - REQ-004.1: Offline invite generation and valid `#payload=` Base64URL fragment packing.
//! - REQ-004.2: Fragment unpacking from diverse URL structures and strict tamper rejection.
//! - REQ-004.3: Joiner auto-drop payload construction, bearer token header, and HTTP dead-drop dispatch.
//! - REQ-004.4 & REQ-004.5: End-to-end loopback handshake, host answer processing, immediate edge severance,
//!   and direct P2P data exchange over the open channel.

use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use std::cell::RefCell;
use std::future::Future;
use std::pin::pin;
use std::rc::Rc;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use zero_client::{
    create_invite_url, create_offline_invite, detect_invite_from_hash, dispatch_drop_post,
    execute_joiner_autodrop, generate_uuid_v4, pack_invite_fragment, simulate_handshake_loopback,
    unpack_invite_fragment, validate_sdp_answer, validate_sdp_offer, verify_invite_payload,
    BootstrapError, DataChannelMessage, HostSignalingSession, InviteError, PeerLifecycleState,
    WebRtcPeer,
};
use zero_protocol::crypto::hmac_sha256::compute_invite_token;
use zero_protocol::{from_json_str, to_json_string, DropPayload, InvitePayload, ProtocolError};

/// Synchronous runner for futures executed during native testing.
fn block_on<F: Future>(fut: F) -> F::Output {
    fn noop_raw_waker() -> RawWaker {
        fn no_op(_: *const ()) {}
        fn clone(_: *const ()) -> RawWaker {
            noop_raw_waker()
        }
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, no_op, no_op, no_op);
        RawWaker::new(std::ptr::null(), &VTABLE)
    }

    let waker = unsafe { Waker::from_raw(noop_raw_waker()) };
    let mut cx = Context::from_waker(&waker);
    let mut pinned = pin!(fut);
    match pinned.as_mut().poll(&mut cx) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("Future remained pending in synchronous test runner"),
    }
}

/// REQ-004.1: Offline invite generation and URL fragment packing.
/// - Host generates offline invite bundle with auto-generated RFC 4122 v4 UUID and HMAC-SHA256 token.
/// - Host generates offline invite with custom UUID.
/// - Packs into URL `#payload=...`.
/// - Validates that Base64URL string decodes to valid JSON containing all expected fields.
#[test]
fn test_req_004_1_offline_invite_generation_and_fragment_packing() {
    block_on(async {
        let host_peer = WebRtcPeer::new();
        let offer = host_peer.create_offer().await.expect("host create offer");
        assert_eq!(validate_sdp_offer(&offer.sdp), Ok(offer.clone()));

        let host_secret = b"host_master_secret_key_32_bytes";
        let host_pubkey = [0x42u8; 32];
        let guest_priv = Some([0x77u8; 32]);
        let room_id = "zero-room-alpha-123";

        // 1. Auto-generated UUID invite
        let payload_auto = create_offline_invite(
            room_id,
            host_secret,
            host_pubkey,
            offer.clone(),
            None,
            guest_priv,
        )
        .expect("create offline invite with auto uuid");

        assert_eq!(payload_auto.do_id, room_id);
        assert_eq!(payload_auto.host_public_key, host_pubkey);
        assert_eq!(payload_auto.guest_private_key, guest_priv);
        assert_eq!(payload_auto.sdp_offer, offer);

        // Verify RFC 4122 v4 UUID format: 8-4-4-4-12 hex chars
        assert_eq!(payload_auto.invite_uuid.len(), 36);
        let uuid_parts: Vec<&str> = payload_auto.invite_uuid.split('-').collect();
        assert_eq!(uuid_parts.len(), 5);
        assert_eq!(uuid_parts[0].len(), 8);
        assert_eq!(uuid_parts[1].len(), 4);
        assert_eq!(uuid_parts[2].len(), 4);
        assert_eq!(uuid_parts[3].len(), 4);
        assert_eq!(uuid_parts[4].len(), 12);
        // Version 4 check
        assert_eq!(&uuid_parts[2][0..1], "4");
        // Variant RFC 4122 check (high nibble 8, 9, a, or b)
        let variant_char = uuid_parts[3].chars().next().unwrap();
        assert!(
            ['8', '9', 'a', 'b'].contains(&variant_char),
            "Variant character was {variant_char}"
        );

        // Verify HMAC-SHA256 token validity
        let expected_token = compute_invite_token(host_secret, &payload_auto.invite_uuid)
            .expect("compute expected token");
        assert_eq!(payload_auto.token, expected_token.hmac_hex);
        assert_eq!(payload_auto.token.len(), 64);
        verify_invite_payload(host_secret, &payload_auto).expect("verify invite payload");

        // 2. Custom UUID invite
        let custom_uuid = "11111111-2222-4333-8444-555555555555";
        let payload_custom = create_offline_invite(
            room_id,
            host_secret,
            host_pubkey,
            offer.clone(),
            Some(custom_uuid),
            None,
        )
        .expect("create offline invite with custom uuid");
        assert_eq!(payload_custom.invite_uuid, custom_uuid);
        assert_eq!(payload_custom.guest_private_key, None);
        verify_invite_payload(host_secret, &payload_custom).expect("verify custom invite");

        // 3. Fragment packing and URL assembly
        let packed_fragment = pack_invite_fragment(&payload_auto).expect("pack invite fragment");
        assert!(
            packed_fragment.starts_with("#payload="),
            "Fragment must start with '#payload='"
        );

        let base_url = "https://zerochat.app/chat";
        let invite_url = create_invite_url(base_url, &payload_auto).expect("create invite url");
        assert_eq!(invite_url, format!("{base_url}{packed_fragment}"));

        // Overwrite existing hash fragment cleanly
        let url_with_existing_hash = "https://zerochat.app/chat#old-stale-hash";
        let replaced_url = create_invite_url(url_with_existing_hash, &payload_auto)
            .expect("replace existing hash in invite url");
        assert_eq!(replaced_url, format!("{base_url}{packed_fragment}"));

        // 4. Base64URL and JSON validation
        let raw_b64 = packed_fragment.strip_prefix("#payload=").unwrap();
        // Ensure no standard base64 characters like '+' or '/' and no padding '='
        assert!(!raw_b64.contains('+'));
        assert!(!raw_b64.contains('/'));
        assert!(!raw_b64.contains('='));

        let decoded_bytes = URL_SAFE_NO_PAD
            .decode(raw_b64.as_bytes())
            .expect("decode unpadded base64url bytes");
        let json_value: serde_json::Value =
            serde_json::from_slice(&decoded_bytes).expect("parse JSON value");

        assert_eq!(json_value["do_id"], room_id);
        assert_eq!(json_value["invite_uuid"], payload_auto.invite_uuid);
        assert_eq!(json_value["token"], payload_auto.token);
        assert_eq!(json_value["sdp_offer"]["sdp"], offer.sdp);
        assert!(json_value["host_public_key"].is_array());
        assert_eq!(json_value["host_public_key"].as_array().unwrap().len(), 32);

        let roundtrip_payload: InvitePayload =
            from_json_str(std::str::from_utf8(&decoded_bytes).unwrap())
                .expect("deserialize InvitePayload");
        assert_eq!(roundtrip_payload, payload_auto);
    });
}

/// REQ-004.2: Fragment unpacking from diverse URL structures and tamper rejection.
/// - Validates unpacking from full URL, fragment-only `#payload=...`, multi-fragment,
///   query-style parameter, and bare Base64URL string.
/// - Tampered HMAC, tampered UUID, corrupt Base64, corrupt JSON, or missing payload parameters
///   are strictly rejected with appropriate errors.
#[test]
fn test_req_004_2_fragment_unpacking_and_tamper_rejection() {
    let host_secret = b"verification_secret_key_32_bytes";
    let dummy_offer = zero_protocol::SdpOffer {
        sdp: "v=0\r\no=- 42 1 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\nm=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\na=candidate:1 1 UDP 2130706431 127.0.0.1 50000 typ host\r\na=end-of-candidates\r\n".to_string(),
    };

    let payload = create_offline_invite(
        "room-omega-9",
        host_secret,
        [0x99u8; 32],
        dummy_offer,
        Some("550e8400-e29b-41d4-a716-446655440000"),
        Some([0x11u8; 32]),
    )
    .expect("create invite payload");

    let packed_fragment = pack_invite_fragment(&payload).expect("pack fragment");
    let raw_b64 = packed_fragment.strip_prefix("#payload=").unwrap();

    // 1. Diverse URL structure unpacking
    // Full URL with path
    let url_1 = format!("https://zerochat.app/join{packed_fragment}");
    let unpacked_1 = unpack_invite_fragment(&url_1).expect("unpack from full URL");
    assert_eq!(unpacked_1, payload);

    // Full URL with query parameters preceding the hash
    let url_2 = format!("https://zerochat.app/join?source=qr&theme=dark{packed_fragment}");
    let unpacked_2 = unpack_invite_fragment(&url_2).expect("unpack with query preceding hash");
    assert_eq!(unpacked_2, payload);

    // Multiple hash parameters separated by '&'
    let url_3 = format!("https://zerochat.app/#channel=lobby&payload={raw_b64}&debug=false");
    let unpacked_3 = unpack_invite_fragment(&url_3).expect("unpack from multi-fragment URL");
    assert_eq!(unpacked_3, payload);

    // Fragment only
    let unpacked_4 = unpack_invite_fragment(&packed_fragment).expect("unpack from fragment only");
    assert_eq!(unpacked_4, payload);

    // Query parameter style (e.g., ?payload=...)
    let url_5 = format!("https://zerochat.app/?payload={raw_b64}");
    let unpacked_5 = unpack_invite_fragment(&url_5).expect("unpack from query parameter URL");
    assert_eq!(unpacked_5, payload);

    let query_only = format!("?payload={raw_b64}");
    let unpacked_5b = unpack_invite_fragment(&query_only).expect("unpack from query string only");
    assert_eq!(unpacked_5b, payload);

    // Bare Base64URL string (direct payload)
    let unpacked_6 = unpack_invite_fragment(raw_b64).expect("unpack from raw base64url string");
    assert_eq!(unpacked_6, payload);

    // Padded Base64URL string
    let json_bytes = to_json_string(&payload).unwrap();
    let padded_b64 = URL_SAFE.encode(json_bytes.as_bytes());
    let url_padded = format!("https://zerochat.app/#payload={padded_b64}");
    let unpacked_7 = unpack_invite_fragment(&url_padded).expect("unpack with padded base64url");
    assert_eq!(unpacked_7, payload);

    // detect_invite_from_hash helper verification
    let detected = detect_invite_from_hash(&packed_fragment).expect("detect from hash");
    assert_eq!(detected, payload);

    // 2. Tampered HMAC verification rejection
    let mut tampered_hmac_payload = payload.clone();
    tampered_hmac_payload.token = hex::encode([0xEEu8; 32]);
    let err = verify_invite_payload(host_secret, &tampered_hmac_payload).unwrap_err();
    assert_eq!(
        err,
        InviteError::Protocol(ProtocolError::InvalidHmacSignature)
    );

    // 3. Tampered UUID verification rejection
    let mut tampered_uuid_payload = payload.clone();
    tampered_uuid_payload.invite_uuid = generate_uuid_v4().expect("generate uuid");
    let err = verify_invite_payload(host_secret, &tampered_uuid_payload).unwrap_err();
    assert_eq!(
        err,
        InviteError::Protocol(ProtocolError::InvalidHmacSignature)
    );

    // 4. Wrong host secret rejection
    let err = verify_invite_payload(b"wrong_secret_key_that_is_32_bytes!", &payload).unwrap_err();
    assert_eq!(
        err,
        InviteError::Protocol(ProtocolError::InvalidHmacSignature)
    );

    // 5. Empty secret rejection
    let err = verify_invite_payload(b"", &payload).unwrap_err();
    assert_eq!(
        err,
        InviteError::Protocol(ProtocolError::InvalidKeyLength(0))
    );

    // 6. Corrupt Base64 input rejection
    let corrupt_b64_url = "https://zerochat.app/#payload=!!!invalid-base64-content!@#$";
    let err = unpack_invite_fragment(corrupt_b64_url).unwrap_err();
    assert!(
        matches!(err, InviteError::Base64(_)),
        "Expected Base64 error, got: {err:?}"
    );

    // 7. Valid Base64 but corrupt JSON rejection
    let non_json_b64 = URL_SAFE_NO_PAD.encode(b"this is plain text, not a valid json object");
    let non_json_url = format!("https://zerochat.app/#payload={non_json_b64}");
    let err = unpack_invite_fragment(&non_json_url).unwrap_err();
    assert!(
        matches!(
            err,
            InviteError::Protocol(ProtocolError::SerializationError(_))
        ),
        "Expected Serialization error, got: {err:?}"
    );

    // 8. Missing payload parameter rejection
    assert_eq!(
        unpack_invite_fragment("https://zerochat.app/#channel=general&token=secret").unwrap_err(),
        InviteError::MissingPayload
    );
    assert_eq!(
        unpack_invite_fragment("#payload=").unwrap_err(),
        InviteError::MissingPayload
    );
    assert_eq!(
        unpack_invite_fragment("").unwrap_err(),
        InviteError::MissingPayload
    );
    assert_eq!(
        detect_invite_from_hash("#").unwrap_err(),
        BootstrapError::NoPayloadInHash
    );
    assert_eq!(
        detect_invite_from_hash("   ").unwrap_err(),
        BootstrapError::NoPayloadInHash
    );
}

/// REQ-004.3: Joiner auto-drop payload construction, bearer token, and HTTP dispatch.
/// - Generates joiner drop request from invite payload.
/// - Verifies bearer token matches `invite.token` and SDP answer is valid.
/// - Verifies dead-drop HTTP dispatch and error handling (empty token 401, error endpoints, empty origin).
#[test]
fn test_req_004_3_joiner_autodrop_payload_and_bearer_headers() {
    block_on(async {
        let host_peer = WebRtcPeer::new();
        let offer = host_peer.create_offer().await.expect("host create offer");

        let host_secret = b"host_secret_joiner_test_32_bytes";
        let invite = create_offline_invite(
            "do-room-drop-101",
            host_secret,
            [0x22u8; 32],
            offer,
            Some("77777777-8888-4999-9000-aaaaaaaaaaaa"),
            None,
        )
        .expect("create invite");

        let joiner_peer = WebRtcPeer::new();
        let joiner_pubkey = [0x55u8; 32];
        let origin_base = "https://edge.zerochat.test";

        // 1. Execute joiner auto-drop
        let drop_payload: DropPayload =
            execute_joiner_autodrop(origin_base, &invite, &joiner_peer, joiner_pubkey)
                .await
                .expect("joiner auto-drop execution");

        // Verify DropPayload fields
        assert_eq!(drop_payload.invite_uuid, invite.invite_uuid);
        assert_eq!(drop_payload.token, invite.token);
        assert_eq!(drop_payload.joiner_public_key, joiner_pubkey);

        // Verify SDP answer format and validity
        assert_eq!(
            validate_sdp_answer(&drop_payload.sdp_answer.sdp),
            Ok(drop_payload.sdp_answer.clone())
        );
        assert!(drop_payload.sdp_answer.sdp.contains("v=0\r\n"));
        assert!(drop_payload
            .sdp_answer
            .sdp
            .contains("m=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\n"));
        assert!(drop_payload.sdp_answer.sdp.contains("a=setup:active\r\n"));
        assert!(drop_payload.sdp_answer.sdp.contains("a=candidate:"));
        assert!(drop_payload
            .sdp_answer
            .sdp
            .ends_with("a=end-of-candidates\r\n"));

        // Joiner peer recorded remote offer and local answer
        assert_eq!(joiner_peer.remote_offer().as_ref(), Some(&invite.sdp_offer));
        assert_eq!(
            joiner_peer.local_answer().as_ref(),
            Some(&drop_payload.sdp_answer)
        );

        // 2. Direct HTTP dispatch verification
        let target_url = format!("{origin_base}/drop/{}/{}", invite.do_id, invite.invite_uuid);
        let status = dispatch_drop_post(&target_url, &invite.token, &drop_payload)
            .await
            .expect("dispatch drop post with valid bearer token");
        assert_eq!(status, 200);

        // Missing/whitespace bearer token must fail with HTTP 401
        let err_empty_token = dispatch_drop_post(&target_url, "   ", &drop_payload)
            .await
            .unwrap_err();
        match err_empty_token {
            BootstrapError::HttpError { status, .. } => assert_eq!(status, 401),
            other => panic!("Expected HttpError 401, got: {other:?}"),
        }

        // Empty endpoint URL must fail with FetchError
        let err_empty_url = dispatch_drop_post("", &invite.token, &drop_payload)
            .await
            .unwrap_err();
        assert_eq!(
            err_empty_url,
            BootstrapError::FetchError("Endpoint URL cannot be empty".to_string())
        );

        // Empty origin base in execute_joiner_autodrop fails with FetchError
        let joiner_peer_err = WebRtcPeer::new();
        let err_empty_origin =
            execute_joiner_autodrop("", &invite, &joiner_peer_err, joiner_pubkey)
                .await
                .unwrap_err();
        assert_eq!(
            err_empty_origin,
            BootstrapError::FetchError("Origin base URL cannot be empty".to_string())
        );

        // Simulated HTTP error responses (e.g., 403 Forbidden on invalid HMAC, 410 Gone on burned token)
        let simulated_403_url = format!("{origin_base}/simulate_error/403");
        let err_403 = dispatch_drop_post(&simulated_403_url, &invite.token, &drop_payload)
            .await
            .unwrap_err();
        match err_403 {
            BootstrapError::HttpError { status, message } => {
                assert_eq!(status, 403);
                assert!(message.contains("403"));
            }
            other => panic!("Expected HttpError 403, got: {other:?}"),
        }

        let simulated_410_url = format!("{origin_base}/simulate_error/410");
        let err_410 = dispatch_drop_post(&simulated_410_url, &invite.token, &drop_payload)
            .await
            .unwrap_err();
        match err_410 {
            BootstrapError::HttpError { status, message } => {
                assert_eq!(status, 410);
                assert!(message.contains("410"));
            }
            other => panic!("Expected HttpError 410, got: {other:?}"),
        }
    });
}

/// REQ-004.4 & REQ-004.5: End-to-end loopback handshake, host answer processing, immediate edge severance,
/// and direct P2P data exchange.
/// - Full handshake between Host and Joiner.
/// - Host receives the dropped answer and completes WebRTC handshake.
/// - Both peers transition to `PeerLifecycleState::Open`.
/// - Host signaling connection reflects `is_edge_severed() == true` immediately upon `Open`.
/// - Bidirectional text and binary messages are exchanged directly over the open channel.
#[test]
fn test_req_004_4_and_5_end_to_end_loopback_and_edge_severance() {
    block_on(async {
        let host_peer = WebRtcPeer::new();
        let joiner_peer = WebRtcPeer::new();

        assert_eq!(host_peer.state(), PeerLifecycleState::Connecting);
        assert_eq!(joiner_peer.state(), PeerLifecycleState::Connecting);

        // Host generates WebRTC offer
        let host_offer = host_peer.create_offer().await.expect("host create offer");

        let host_secret_bytes = [0x33u8; 32];
        let do_id = "do-e2e-session-001";
        let invite = create_offline_invite(
            do_id,
            &host_secret_bytes,
            [0x88u8; 32],
            host_offer,
            None,
            Some([0x99u8; 32]),
        )
        .expect("create invite");

        // Host creates signaling session to /host/{do_id}
        let mut host_session =
            HostSignalingSession::new(do_id, host_secret_bytes, host_peer.clone());
        assert_eq!(host_session.do_id(), do_id);
        assert_eq!(host_session.host_secret(), &host_secret_bytes);
        assert!(!host_session.is_edge_severed());

        // Connect host signaling
        host_session
            .connect("wss://edge.zerochat.test")
            .expect("host connect signaling");
        assert!(!host_session.is_edge_severed());

        // Joiner unpacks invite URL, generates answer, and executes auto-drop
        let packed_url = create_invite_url("https://zerochat.app/#", &invite).unwrap();
        let detected_invite = detect_invite_from_hash(&packed_url).unwrap();
        assert_eq!(detected_invite, invite);

        let joiner_pubkey = [0x55u8; 32];
        let drop_payload = execute_joiner_autodrop(
            "https://edge.zerochat.test",
            &detected_invite,
            &joiner_peer,
            joiner_pubkey,
        )
        .await
        .expect("execute joiner autodrop");

        // Host receives the dropped answer over the signaling channel
        let drop_json = to_json_string(&drop_payload).expect("serialize drop payload");
        let parsed_drop = host_session
            .handle_incoming_drop_message(&drop_json)
            .await
            .expect("host handle incoming drop message");
        assert_eq!(parsed_drop, drop_payload);

        // Establish in-memory loopback pair between Host and Joiner
        WebRtcPeer::pair_for_testing(&host_peer, &joiner_peer).expect("pair host and joiner");

        // REQ-004.4: Both peers must be Open
        assert_eq!(host_peer.state(), PeerLifecycleState::Open);
        assert_eq!(joiner_peer.state(), PeerLifecycleState::Open);

        // REQ-004.5: Host signaling socket must be severed immediately upon Open transition
        assert!(
            host_session.is_edge_severed(),
            "Host signaling edge connection must be severed upon reaching Open state"
        );

        // Verify Debug representation redacts secret
        let debug_repr = format!("{host_session:?}");
        assert!(debug_repr.contains("[REDACTED]"));
        assert!(!debug_repr.contains("33, 33, 33"));

        // Direct P2P data exchange over DataChannel
        let host_received_text = Rc::new(RefCell::new(Vec::<String>::new()));
        let joiner_received_text = Rc::new(RefCell::new(Vec::<String>::new()));
        let host_received_bin = Rc::new(RefCell::new(Vec::<Vec<u8>>::new()));
        let joiner_received_bin = Rc::new(RefCell::new(Vec::<Vec<u8>>::new()));

        {
            let text_box = host_received_text.clone();
            host_peer.on_text_message(move |text| {
                text_box.borrow_mut().push(text.to_string());
            });
        }
        {
            let text_box = joiner_received_text.clone();
            joiner_peer.on_text_message(move |text| {
                text_box.borrow_mut().push(text.to_string());
            });
        }
        {
            let bin_box = host_received_bin.clone();
            host_peer.on_binary_message(move |data| {
                bin_box.borrow_mut().push(data.to_vec());
            });
        }
        {
            let bin_box = joiner_received_bin.clone();
            joiner_peer.on_binary_message(move |data| {
                bin_box.borrow_mut().push(data.to_vec());
            });
        }

        // Host -> Joiner text message
        let host_msg = "Hello Joiner! Zero-Chat P2P link established.";
        host_peer.send_text(host_msg).expect("host send_text");
        assert_eq!(*joiner_received_text.borrow(), vec![host_msg]);

        // Joiner -> Host text message
        let joiner_msg = "Hello Host! Direct WebRTC DataChannel active.";
        joiner_peer.send_text(joiner_msg).expect("joiner send_text");
        assert_eq!(*host_received_text.borrow(), vec![joiner_msg]);

        // Host -> Joiner binary packet
        let binary_payload_1 = vec![0xCA, 0xFE, 0xBA, 0xBE, 0x01, 0x02];
        host_peer
            .send_binary(&binary_payload_1)
            .expect("host send_binary");
        assert_eq!(
            *joiner_received_bin.borrow(),
            vec![binary_payload_1.clone()]
        );

        // Joiner -> Host binary packet
        let binary_payload_2 = vec![0xDE, 0xAD, 0xBE, 0xEF, 0xFF];
        joiner_peer
            .send_binary(&binary_payload_2)
            .expect("joiner send_binary");
        assert_eq!(*host_received_bin.borrow(), vec![binary_payload_2.clone()]);

        // Verify outbound messages recorded
        assert_eq!(
            host_peer.outbound_messages(),
            vec![
                DataChannelMessage::Text(host_msg.to_string()),
                DataChannelMessage::Binary(binary_payload_1),
            ]
        );
        assert_eq!(
            joiner_peer.outbound_messages(),
            vec![
                DataChannelMessage::Text(joiner_msg.to_string()),
                DataChannelMessage::Binary(binary_payload_2),
            ]
        );

        // Also verify simulate_handshake_loopback helper directly
        let host_peer_2 = WebRtcPeer::new();
        let joiner_peer_2 = WebRtcPeer::new();
        let offer_2 = host_peer_2.create_offer().await.unwrap();
        let invite_2 = create_offline_invite(
            "do-helper-test",
            b"helper_secret_key_32_bytes_long!",
            [0x11u8; 32],
            offer_2,
            None,
            None,
        )
        .unwrap();

        simulate_handshake_loopback(&host_peer_2, &joiner_peer_2, &invite_2, [0x22u8; 32])
            .await
            .expect("simulate_handshake_loopback helper execution");
        assert_eq!(host_peer_2.state(), PeerLifecycleState::Open);
        assert_eq!(joiner_peer_2.state(), PeerLifecycleState::Open);
    });
}
