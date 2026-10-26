use zero_protocol::crypto::{
    burner::BurnerKeybundle,
    chacha20poly1305::{
        generate_nonce, open_payload, open_with_shared_secret, seal_payload,
        seal_payload_with_nonce, seal_with_shared_secret,
    },
    ed25519::{verify_ed25519_signature, Ed25519Keypair},
    hmac_sha256::{
        compute_hmac_sha256, compute_invite_token, verify_hmac_sha256, verify_invite_token,
    },
    uuid::{generate_random_bytes_16, generate_uuid_v4},
    x25519::{derive_shared_secret, X25519Keypair},
    KEY_LEN, NONCE_LEN, SIGNATURE_LEN, TAG_LEN,
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
        host_signing_key: Some([0x33; 32]),
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
    assert_eq!(decoded.host_signing_key, None);
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
        joiner_signing_key: Some([0x44; 32]),
        sealed_sdp_answer: Some(SealedFrame {
            nonce: [0x01; 12],
            ciphertext: vec![1, 2, 3, 4],
            signature: None,
        }),
        signature: Some(vec![0x55; 64]),
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
    assert_eq!(decoded.joiner_signing_key, None);
    assert_eq!(decoded.sealed_sdp_answer, None);
    assert_eq!(decoded.signature, None);
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
            signature: None,
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
                signing_key: Some([0x55; 32]),
                signature: Some(vec![0x66; 64]),
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
                    signature: None,
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
                    signature: None,
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
                    signature: None,
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

// ============================================================================
// REQ-046: Option B Burner Keybundle, Ed25519, Blind Dead-Drop & Extensions
// ============================================================================

#[test]
fn test_burner_keybundle_generation_and_zeroize() {
    let mut bundle = BurnerKeybundle::generate().expect("generate BurnerKeybundle");

    // Public keys are populated and non-zero
    assert_ne!(bundle.x25519_public_key(), [0u8; KEY_LEN]);
    assert_ne!(bundle.ed25519_public_key(), [0u8; KEY_LEN]);
    assert_eq!(
        bundle.x25519_public_key(),
        *bundle.x25519.public_key_bytes()
    );
    assert_eq!(
        bundle.ed25519_public_key(),
        *bundle.ed25519.public_key_bytes()
    );

    // Sign message with Ed25519
    let msg = b"Ephemeral identity proof";
    let sig = bundle.ed25519.sign(msg);
    assert_eq!(sig.len(), SIGNATURE_LEN);

    // Verify signature with correct public key
    verify_ed25519_signature(&bundle.ed25519_public_key(), msg, &sig)
        .expect("signature must verify against keybundle public key");

    // Verify signature fails with wrong public key
    let wrong_key = [0x77u8; KEY_LEN];
    let err_wrong_key = verify_ed25519_signature(&wrong_key, msg, &sig).unwrap_err();
    assert_eq!(err_wrong_key, ProtocolError::InvalidHmacSignature);

    // Verify signature fails with altered message
    let err_tampered_msg =
        verify_ed25519_signature(&bundle.ed25519_public_key(), b"Tampered message", &sig)
            .unwrap_err();
    assert_eq!(err_tampered_msg, ProtocolError::InvalidHmacSignature);

    // Test zeroization
    bundle.zeroize();
    assert_eq!(bundle.x25519.private_key, [0u8; KEY_LEN]);
    assert_eq!(bundle.ed25519.seed, [0u8; KEY_LEN]);
}

#[test]
fn test_blind_dead_drop_sdp_encryption_and_tamper_rejection() {
    // 1. Host creates ephemeral keybundle and derives invite parameters
    let host_bundle = BurnerKeybundle::generate().expect("generate host keybundle");
    let host_pub = host_bundle.x25519_public_key();

    // 2. Joiner opens invite URL with host_pub, generates joiner keybundle
    let joiner_bundle = BurnerKeybundle::generate().expect("generate joiner keybundle");
    let joiner_pub = joiner_bundle.x25519_public_key();

    // Pairwise shared secret derivation
    let joiner_shared_secret =
        derive_shared_secret(joiner_bundle.x25519.private_key_bytes(), &host_pub);
    let host_shared_secret =
        derive_shared_secret(host_bundle.x25519.private_key_bytes(), &joiner_pub);
    assert_eq!(joiner_shared_secret, host_shared_secret);

    // 3. Joiner seals WebRTC SDP answer with shared secret
    let raw_sdp = "v=0\r\no=- 4829104 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\na=sendrecv";
    let sealed_frame = seal_with_shared_secret(&joiner_shared_secret, raw_sdp.as_bytes())
        .expect("seal SDP with shared secret");

    // Construct DropPayload
    let drop = DropPayload {
        invite_uuid: "550e8400-e29b-41d4-a716-446655440000".to_string(),
        token: "feedbeef".repeat(8),
        sdp_answer: SdpAnswer {
            sdp: "SEALED".to_string(),
        },
        joiner_public_key: joiner_pub,
        joiner_peer_id: Some("joiner-bob".to_string()),
        joiner_signing_key: Some(joiner_bundle.ed25519_public_key()),
        sealed_sdp_answer: Some(sealed_frame.clone()),
        signature: None,
    };

    // 4. Host receives DropPayload and decrypts sealed_sdp_answer
    let host_derived = derive_shared_secret(
        host_bundle.x25519.private_key_bytes(),
        &drop.joiner_public_key,
    );
    assert_eq!(host_derived, host_shared_secret);

    let decrypted_bytes =
        open_with_shared_secret(&host_derived, drop.sealed_sdp_answer.as_ref().unwrap())
            .expect("host must decrypt sealed SDP successfully");
    let decrypted_sdp = String::from_utf8(decrypted_bytes).expect("valid UTF-8 SDP");
    assert_eq!(decrypted_sdp, raw_sdp);

    // 5. Tamper rejection - corrupted ciphertext returns DecryptionError
    let mut tampered_frame = sealed_frame.clone();
    tampered_frame.ciphertext[0] ^= 0xff;
    let err = open_with_shared_secret(&host_derived, &tampered_frame).unwrap_err();
    assert!(matches!(err, ProtocolError::DecryptionError(_)));

    // 6. Tamper rejection - corrupted nonce returns DecryptionError
    let mut tampered_nonce = sealed_frame;
    tampered_nonce.nonce[0] ^= 0x55;
    let err_nonce = open_with_shared_secret(&host_derived, &tampered_nonce).unwrap_err();
    assert!(matches!(err_nonce, ProtocolError::DecryptionError(_)));
}

#[test]
fn test_signed_sealed_frame_authenticity() {
    let alice_ed = Ed25519Keypair::generate().expect("generate Alice Ed25519");
    let mallory_ed = Ed25519Keypair::generate().expect("generate Mallory Ed25519");
    let key = [0x5au8; KEY_LEN];

    let plaintext = b"Authenticated P2P payload";
    let mut sealed = seal_payload(&key, plaintext).expect("seal payload");

    // Sign (nonce || ciphertext) with Alice's signing key
    let mut to_sign = Vec::with_capacity(NONCE_LEN + sealed.ciphertext.len());
    to_sign.extend_from_slice(&sealed.nonce);
    to_sign.extend_from_slice(&sealed.ciphertext);

    let sig = alice_ed.sign(&to_sign);
    sealed.signature = Some(sig.to_vec());

    // Alice's public key verifies successfully
    let sig_arr: [u8; SIGNATURE_LEN] = sealed
        .signature
        .as_ref()
        .unwrap()
        .as_slice()
        .try_into()
        .unwrap();
    verify_ed25519_signature(alice_ed.public_key_bytes(), &to_sign, &sig_arr)
        .expect("Alice's signature must verify");

    // Mallory's public key fails verification
    let err_mallory =
        verify_ed25519_signature(mallory_ed.public_key_bytes(), &to_sign, &sig_arr).unwrap_err();
    assert_eq!(err_mallory, ProtocolError::InvalidHmacSignature);

    // Tampered payload fails verification
    let mut tampered_to_sign = to_sign.clone();
    tampered_to_sign[0] ^= 0x01;
    let err_tamper =
        verify_ed25519_signature(alice_ed.public_key_bytes(), &tampered_to_sign, &sig_arr)
            .unwrap_err();
    assert_eq!(err_tamper, ProtocolError::InvalidHmacSignature);
}

#[test]
fn test_backward_compatible_envelopes() {
    // 1. Legacy InvitePayload without host_signing_key
    let legacy_invite = r#"{
        "do_id": "do-xyz",
        "invite_uuid": "inv-001",
        "token": "tok-001",
        "sdp_offer": { "sdp": "v=0\r\n" },
        "host_public_key": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]
    }"#;
    let decoded_invite: InvitePayload =
        from_json_str(legacy_invite).expect("deserialize legacy InvitePayload");
    assert_eq!(decoded_invite.host_signing_key, None);
    assert_eq!(decoded_invite.guest_private_key, None);
    assert_eq!(decoded_invite.host_peer_id, None);
    assert_eq!(decoded_invite.assigned_peer_id, None);

    // 2. Legacy DropPayload without joiner_signing_key, sealed_sdp_answer, signature
    let legacy_drop = r#"{
        "invite_uuid": "inv-001",
        "token": "tok-001",
        "sdp_answer": { "sdp": "v=0\r\n" },
        "joiner_public_key": [1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1]
    }"#;
    let decoded_drop: DropPayload =
        from_json_str(legacy_drop).expect("deserialize legacy DropPayload");
    assert_eq!(decoded_drop.joiner_signing_key, None);
    assert_eq!(decoded_drop.sealed_sdp_answer, None);
    assert_eq!(decoded_drop.signature, None);
    assert_eq!(decoded_drop.joiner_peer_id, None);

    // 3. Legacy SealedFrame without signature
    let legacy_sealed = r#"{
        "nonce": [1,2,3,4,5,6,7,8,9,10,11,12],
        "ciphertext": [99, 100, 101]
    }"#;
    let decoded_sealed: SealedFrame =
        from_json_str(legacy_sealed).expect("deserialize legacy SealedFrame");
    assert_eq!(decoded_sealed.signature, None);

    // 4. Legacy PeerJoined without signing_key and signature
    let legacy_peer_joined = r#"{
        "PeerJoined": {
            "peer_id": "legacy-peer-1",
            "public_key": [2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2]
        }
    }"#;
    let decoded_signal: MeshRelaySignal =
        from_json_str(legacy_peer_joined).expect("deserialize legacy PeerJoined");
    if let MeshRelaySignal::PeerJoined {
        peer_id,
        public_key,
        signing_key,
        signature,
    } = decoded_signal
    {
        assert_eq!(peer_id, "legacy-peer-1");
        assert_eq!(public_key, [2u8; 32]);
        assert_eq!(signing_key, None);
        assert_eq!(signature, None);
    } else {
        panic!("expected PeerJoined variant");
    }
}

#[test]
fn test_generate_random_bytes_16_and_uuid_v4() {
    let bytes1 = generate_random_bytes_16().expect("generate 16 random bytes");
    let bytes2 = generate_random_bytes_16().expect("generate second 16 random bytes");
    assert_ne!(bytes1, [0u8; 16]);
    assert_ne!(bytes1, bytes2);

    let uuid1 = generate_uuid_v4().expect("generate uuid v4");
    let uuid2 = generate_uuid_v4().expect("generate second uuid v4");
    assert_ne!(uuid1, uuid2);
    assert_eq!(uuid1.len(), 36);

    // Check UUID v4 format: 8-4-4-4-12
    let parts: Vec<&str> = uuid1.split('-').collect();
    assert_eq!(parts.len(), 5);
    assert_eq!(parts[0].len(), 8);
    assert_eq!(parts[1].len(), 4);
    assert_eq!(parts[2].len(), 4);
    assert_eq!(parts[3].len(), 4);
    assert_eq!(parts[4].len(), 12);

    // Version 4 check: 13th character is '4' (parts[2].chars().next())
    assert!(parts[2].starts_with('4'));
    // Variant 1 check: 17th character is '8', '9', 'a', or 'b'
    let variant_char = parts[3].chars().next().unwrap();
    assert!(matches!(variant_char, '8' | '9' | 'a' | 'b'));
}
