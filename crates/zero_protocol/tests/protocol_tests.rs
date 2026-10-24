use zero_protocol::crypto::{
    chacha20poly1305::{generate_nonce, open_payload, seal_payload, seal_payload_with_nonce},
    hmac_sha256::{
        compute_hmac_sha256, compute_invite_token, verify_hmac_sha256, verify_invite_token,
    },
    x25519::{derive_shared_secret, X25519Keypair},
    KEY_LEN, NONCE_LEN, TAG_LEN,
};
use zero_protocol::envelope::{
    from_json_bytes, from_json_str, to_json_bytes, to_json_string, ChatMessage, DropPayload,
    InvitePayload, MeshRelayMessage, MeshRelaySignal, ProtocolError, SdpAnswer, SdpOffer,
    SealedFrame,
};

// ============================================================================
// REQ-001.2: Protocol Envelope Serialization & Deserialization
// ============================================================================

#[test]
fn test_invite_payload_roundtrip_serialization() {
    let payload = InvitePayload {
        do_id: "do-namespace-123".to_string(),
        invite_uuid: "550e8400-e29b-41d4-a716-446655440000".to_string(),
        token: "deadbeef".repeat(8),
        sdp_offer: SdpOffer {
            sdp: "v=0\r\no=- 12345 2 IN IP4 127.0.0.1\r\n".to_string(),
        },
        host_public_key: [0x11; 32],
        guest_private_key: Some([0x22; 32]),
        host_peer_id: Some("host-peer-alice".to_string()),
        assigned_peer_id: Some("guest-peer-bob".to_string()),
    };

    let json_str = to_json_string(&payload).expect("serialize to JSON string");
    assert!(json_str.contains("\"host_peer_id\":\"host-peer-alice\""));
    assert!(json_str.contains("\"assigned_peer_id\":\"guest-peer-bob\""));
    let decoded_str: InvitePayload = from_json_str(&json_str).expect("deserialize from JSON str");
    assert_eq!(payload, decoded_str);

    let json_bytes = to_json_bytes(&payload).expect("serialize to JSON bytes");
    let decoded_bytes: InvitePayload =
        from_json_bytes(&json_bytes).expect("deserialize from JSON bytes");
    assert_eq!(payload, decoded_bytes);
}

#[test]
fn test_invite_payload_backward_compatibility() {
    let legacy_json = format!(
        r#"{{
        "do_id": "do-legacy",
        "invite_uuid": "legacy-uuid",
        "token": "legacy-token",
        "sdp_offer": {{ "sdp": "v=0\r\n" }},
        "host_public_key": {:?}
    }}"#,
        vec![0u8; 32]
    );
    let decoded: InvitePayload = from_json_str(&legacy_json).expect("deserialize legacy payload");
    assert_eq!(decoded.guest_private_key, None);
    assert_eq!(decoded.host_peer_id, None);
    assert_eq!(decoded.assigned_peer_id, None);
}

#[test]
fn test_drop_payload_roundtrip_serialization() {
    let payload = DropPayload {
        invite_uuid: "550e8400-e29b-41d4-a716-446655440000".to_string(),
        token: "cafebabe".repeat(8),
        sdp_answer: SdpAnswer {
            sdp: "v=0\r\no=- 67890 2 IN IP4 127.0.0.1\r\n".to_string(),
        },
        joiner_public_key: [0x33; 32],
        joiner_peer_id: Some("joiner-peer-charlie".to_string()),
    };

    let json_str = to_json_string(&payload).expect("serialize DropPayload to string");
    assert!(json_str.contains("\"joiner_peer_id\":\"joiner-peer-charlie\""));
    let decoded_str: DropPayload = from_json_str(&json_str).expect("deserialize DropPayload");
    assert_eq!(payload, decoded_str);

    let json_bytes = to_json_bytes(&payload).expect("serialize DropPayload to bytes");
    let decoded_bytes: DropPayload =
        from_json_bytes(&json_bytes).expect("deserialize DropPayload from bytes");
    assert_eq!(payload, decoded_bytes);
}

#[test]
fn test_drop_payload_backward_compatibility() {
    let legacy_json = format!(
        r#"{{
        "invite_uuid": "legacy-drop-uuid",
        "token": "legacy-drop-token",
        "sdp_answer": {{ "sdp": "v=0\r\n" }},
        "joiner_public_key": {:?}
    }}"#,
        vec![1u8; 32]
    );
    let decoded: DropPayload =
        from_json_str(&legacy_json).expect("deserialize legacy drop payload");
    assert_eq!(decoded.joiner_peer_id, None);
}

#[test]
fn test_chat_message_roundtrip_serialization() {
    let msg = ChatMessage {
        message_uuid: "msg-001".to_string(),
        sender_id: "peer-alice".to_string(),
        timestamp_ms: 1_729_114_800_000,
        body: "Hello ephemeral mesh!".to_string(),
        sealed: Some(SealedFrame {
            nonce: [0x01; 12],
            ciphertext: vec![0xaa, 0xbb, 0xcc, 0xdd],
        }),
    };

    let json_str = to_json_string(&msg).expect("serialize ChatMessage to string");
    let decoded_str: ChatMessage = from_json_str(&json_str).expect("deserialize ChatMessage");
    assert_eq!(msg, decoded_str);

    let json_bytes = to_json_bytes(&msg).expect("serialize ChatMessage to bytes");
    let decoded_bytes: ChatMessage =
        from_json_bytes(&json_bytes).expect("deserialize ChatMessage from bytes");
    assert_eq!(msg, decoded_bytes);
}

#[test]
fn test_mesh_relay_message_roundtrip_serialization() {
    let variants = vec![
        MeshRelayMessage {
            frame_id: "frame-1".to_string(),
            ttl: 4,
            signal: MeshRelaySignal::PeerJoined {
                peer_id: "peer-charlie".to_string(),
                public_key: [0x44; 32],
            },
        },
        MeshRelayMessage {
            frame_id: "frame-2".to_string(),
            ttl: 3,
            signal: MeshRelaySignal::RelaySdpOffer {
                from_peer_id: "peer-alice".to_string(),
                to_peer_id: "peer-charlie".to_string(),
                sealed_offer: SealedFrame {
                    nonce: [0x02; 12],
                    ciphertext: vec![1, 2, 3, 4],
                },
            },
        },
        MeshRelayMessage {
            frame_id: "frame-3".to_string(),
            ttl: 3,
            signal: MeshRelaySignal::RelaySdpAnswer {
                from_peer_id: "peer-charlie".to_string(),
                to_peer_id: "peer-alice".to_string(),
                sealed_answer: SealedFrame {
                    nonce: [0x03; 12],
                    ciphertext: vec![5, 6, 7, 8],
                },
            },
        },
        MeshRelayMessage {
            frame_id: "frame-4".to_string(),
            ttl: 5,
            signal: MeshRelaySignal::BroadcastChat {
                message_uuid: "chat-uuid-99".to_string(),
                origin_peer_id: "peer-bob".to_string(),
                ttl: 5,
                sealed_message: SealedFrame {
                    nonce: [0x04; 12],
                    ciphertext: vec![9, 10, 11, 12],
                },
            },
        },
    ];

    for relay_msg in variants {
        let json_str = to_json_string(&relay_msg).expect("serialize MeshRelayMessage");
        let decoded: MeshRelayMessage =
            from_json_str(&json_str).expect("deserialize MeshRelayMessage");
        assert_eq!(relay_msg, decoded);

        let bytes = to_json_bytes(&relay_msg).expect("serialize MeshRelayMessage to bytes");
        let decoded_bytes: MeshRelayMessage =
            from_json_bytes(&bytes).expect("deserialize MeshRelayMessage from bytes");
        assert_eq!(relay_msg, decoded_bytes);
    }
}

#[test]
fn test_malformed_json_returns_serialization_error() {
    let err = from_json_str::<InvitePayload>("{ invalid json ").unwrap_err();
    assert!(matches!(err, ProtocolError::SerializationError(_)));

    let err_bytes = from_json_bytes::<DropPayload>(b"not-a-valid-json-object").unwrap_err();
    assert!(matches!(err_bytes, ProtocolError::SerializationError(_)));
}

// ============================================================================
// REQ-001.3: HMAC-SHA256 Token Generation & Constant-Time Verification
// ============================================================================

#[test]
fn test_hmac_sha256_rfc4231_test_vector_1() {
    let key = [0x0bu8; 20];
    let data = b"Hi There";
    let expected_hex = "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7";

    let mac = compute_hmac_sha256(&key, data).expect("compute HMAC-SHA256");
    assert_eq!(hex::encode(mac), expected_hex);

    let expected_bytes = hex::decode(expected_hex).unwrap();
    verify_hmac_sha256(&key, data, &expected_bytes).expect("verify RFC 4231 test vector");
}

#[test]
fn test_hmac_sha256_invite_token_roundtrip_and_tamper_rejection() {
    let host_secret = b"super-secret-ephemeral-host-key-material";
    let invite_uuid = "550e8400-e29b-41d4-a716-446655440000";

    let token = compute_invite_token(host_secret, invite_uuid).expect("compute invite token");
    assert_eq!(token.invite_uuid, invite_uuid);
    assert_eq!(token.hmac_hex.len(), 64);

    // Valid verification succeeds
    verify_invite_token(host_secret, invite_uuid, &token.hmac_hex)
        .expect("valid invite token must verify");

    // Tampered invite_uuid fails with InvalidHmacSignature
    let err_tampered_uuid = verify_invite_token(
        host_secret,
        "550e8400-e29b-41d4-a716-446655440001",
        &token.hmac_hex,
    )
    .unwrap_err();
    assert_eq!(err_tampered_uuid, ProtocolError::InvalidHmacSignature);

    // Tampered hex signature fails with InvalidHmacSignature
    let mut tampered_hex = token.hmac_hex.clone();
    let replacement = if tampered_hex.ends_with('0') {
        "1"
    } else {
        "0"
    };
    tampered_hex.replace_range(63..64, replacement);
    let err_tampered_sig =
        verify_invite_token(host_secret, invite_uuid, &tampered_hex).unwrap_err();
    assert_eq!(err_tampered_sig, ProtocolError::InvalidHmacSignature);

    // Malformed non-hex string fails with InvalidHmacSignature
    let err_non_hex =
        verify_invite_token(host_secret, invite_uuid, "not-valid-hex-string!!").unwrap_err();
    assert_eq!(err_non_hex, ProtocolError::InvalidHmacSignature);

    // Wrong host secret fails with InvalidHmacSignature
    let err_wrong_secret =
        verify_invite_token(b"wrong-host-secret", invite_uuid, &token.hmac_hex).unwrap_err();
    assert_eq!(err_wrong_secret, ProtocolError::InvalidHmacSignature);

    // Empty secret returns InvalidKeyLength(0)
    assert_eq!(
        compute_hmac_sha256(&[], b"msg").unwrap_err(),
        ProtocolError::InvalidKeyLength(0)
    );
    assert_eq!(
        verify_invite_token(&[], invite_uuid, &token.hmac_hex).unwrap_err(),
        ProtocolError::InvalidKeyLength(0)
    );
}

// ============================================================================
// REQ-001.4: X25519 Key Exchange & ChaCha20-Poly1305 AEAD Payload Sealing
// ============================================================================

#[test]
fn test_x25519_diffie_hellman_shared_secret_agreement() {
    let alice = X25519Keypair::generate().expect("generate Alice X25519 keypair");
    let bob = X25519Keypair::generate().expect("generate Bob X25519 keypair");

    assert_ne!(alice.public_key, [0u8; KEY_LEN]);
    assert_ne!(bob.public_key, [0u8; KEY_LEN]);
    assert_ne!(alice.public_key, bob.public_key);

    let alice_shared = alice.diffie_hellman(&bob.public_key);
    let bob_shared = bob.diffie_hellman(&alice.public_key);

    assert_eq!(alice_shared, bob_shared);
    assert_ne!(alice_shared, [0u8; KEY_LEN]);

    // Also verify standalone derive_shared_secret helper and deterministic from_private_key
    let alice_reconstructed = X25519Keypair::from_private_key(*alice.private_key_bytes());
    assert_eq!(
        alice_reconstructed.public_key_bytes(),
        alice.public_key_bytes()
    );
    assert_eq!(
        derive_shared_secret(alice.private_key_bytes(), bob.public_key_bytes()),
        alice_shared
    );
}

#[test]
fn test_chacha20poly1305_aead_seal_and_open_roundtrip() {
    let alice = X25519Keypair::generate().expect("generate Alice keypair");
    let bob = X25519Keypair::generate().expect("generate Bob keypair");
    let shared_key = alice.diffie_hellman(&bob.public_key);

    let plaintext = b"Confidential P2P signaling SDP offer & chat payload";
    let sealed = seal_payload(&shared_key, plaintext).expect("seal payload");

    assert_eq!(sealed.nonce.len(), NONCE_LEN);
    assert_ne!(sealed.nonce, [0u8; NONCE_LEN]);
    assert_eq!(sealed.ciphertext.len(), plaintext.len() + TAG_LEN);

    let opened = open_payload(&shared_key, &sealed).expect("open valid sealed frame");
    assert_eq!(opened.as_slice(), plaintext);
}

#[test]
fn test_chacha20poly1305_rejects_tampered_ciphertext_nonce_and_wrong_key() {
    let key = [0x42u8; KEY_LEN];
    let nonce = generate_nonce().expect("generate 12-byte nonce");
    let plaintext = b"Authenticated message frame";

    let sealed =
        seal_payload_with_nonce(&key, &nonce, plaintext).expect("seal with explicit nonce");

    // 1. Tamper with ciphertext body
    let mut tampered_ciphertext = sealed.clone();
    tampered_ciphertext.ciphertext[0] ^= 0x01;
    let err_ct = open_payload(&key, &tampered_ciphertext).unwrap_err();
    assert!(matches!(err_ct, ProtocolError::DecryptionError(_)));

    // 2. Tamper with Poly1305 authentication tag (last byte)
    let mut tampered_tag = sealed.clone();
    let last_idx = tampered_tag.ciphertext.len() - 1;
    tampered_tag.ciphertext[last_idx] ^= 0x80;
    let err_tag = open_payload(&key, &tampered_tag).unwrap_err();
    assert!(matches!(err_tag, ProtocolError::DecryptionError(_)));

    // 3. Tamper with 12-byte nonce
    let mut tampered_nonce = sealed.clone();
    tampered_nonce.nonce[0] ^= 0xff;
    let err_nonce = open_payload(&key, &tampered_nonce).unwrap_err();
    assert!(matches!(err_nonce, ProtocolError::DecryptionError(_)));

    // 4. Wrong 32-byte key
    let wrong_key = [0x99u8; KEY_LEN];
    let err_key = open_payload(&wrong_key, &sealed).unwrap_err();
    assert!(matches!(err_key, ProtocolError::DecryptionError(_)));
}
