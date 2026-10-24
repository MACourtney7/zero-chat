//! Offline HMAC invite generator, URL fragment packaging, and verification.

use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use std::fmt;
use zero_protocol::crypto::hmac_sha256::{compute_invite_token, verify_invite_token};
use zero_protocol::crypto::x25519::X25519Keypair;
use zero_protocol::{from_json_str, to_json_string, InvitePayload, ProtocolError, SdpOffer};

/// Errors encountered during invite creation, fragment packaging, unpacking, or verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InviteError {
    Protocol(ProtocolError),
    Base64(String),
    MissingPayload,
    InvalidFragment(String),
    Serialization(String),
}

impl fmt::Display for InviteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Protocol(e) => write!(f, "Protocol error: {e}"),
            Self::Base64(e) => write!(f, "Base64 decode error: {e}"),
            Self::MissingPayload => write!(f, "Fragment is missing payload parameter"),
            Self::InvalidFragment(e) => write!(f, "Invalid fragment: {e}"),
            Self::Serialization(e) => write!(f, "Serialization error: {e}"),
        }
    }
}

impl std::error::Error for InviteError {}

impl From<ProtocolError> for InviteError {
    fn from(err: ProtocolError) -> Self {
        Self::Protocol(err)
    }
}

/// Generates a valid RFC 4122 v4 UUID (`xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx`)
/// using cryptographically secure random bytes from `zero_protocol`'s `X25519Keypair::generate`.
pub fn generate_uuid_v4() -> Result<String, InviteError> {
    let keypair = X25519Keypair::generate()?;
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&keypair.private_key_bytes()[..16]);
    // Set version to 4 (0100 in high 4 bits of time_hi_and_version)
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    // Set variant to RFC 4122 (10 in high 2 bits of clock_seq_hi_and_reserved)
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    let hex_str = hex::encode(bytes);
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex_str[0..8],
        &hex_str[8..12],
        &hex_str[12..16],
        &hex_str[16..20],
        &hex_str[20..32],
    ))
}

/// Generates an offline invite payload with an HMAC-SHA256 verification token.
///
/// If `custom_invite_uuid` is `Some`, that UUID is used; otherwise a new RFC 4122 v4 UUID
/// is generated automatically.
pub fn create_offline_invite(
    do_id: &str,
    host_secret: &[u8],
    host_public_key: [u8; 32],
    sdp_offer: SdpOffer,
    custom_invite_uuid: Option<&str>,
    guest_private_key: Option<[u8; 32]>,
) -> Result<InvitePayload, InviteError> {
    let invite_uuid = match custom_invite_uuid {
        Some(uuid) => uuid.to_string(),
        None => generate_uuid_v4()?,
    };

    let token = compute_invite_token(host_secret, &invite_uuid)?;

    Ok(InvitePayload {
        do_id: do_id.to_string(),
        invite_uuid,
        token: token.hmac_hex,
        sdp_offer,
        host_public_key,
        guest_private_key,
        host_peer_id: None,
        assigned_peer_id: None,
    })
}

/// Serializes `payload` to JSON and encodes it as a URL-safe Base64 (unpadded) fragment string.
///
/// Returns format: `#payload=<base64url>`
pub fn pack_invite_fragment(payload: &InvitePayload) -> Result<String, InviteError> {
    let json = to_json_string(payload)?;
    let encoded = URL_SAFE_NO_PAD.encode(json.as_bytes());
    Ok(format!("#payload={}", encoded))
}

/// Appends the packed `#payload=...` fragment to `base_url`, stripping any existing fragment.
pub fn create_invite_url(base_url: &str, payload: &InvitePayload) -> Result<String, InviteError> {
    let base_without_hash = base_url.split('#').next().unwrap_or(base_url);
    let fragment = pack_invite_fragment(payload)?;
    Ok(format!("{}{}", base_without_hash, fragment))
}

/// Extracts the raw Base64URL string from a URL, fragment, query parameter, or bare Base64URL string.
fn extract_raw_payload(fragment_or_url: &str) -> Result<&str, InviteError> {
    let trimmed = fragment_or_url.trim();
    if trimmed.is_empty() {
        return Err(InviteError::MissingPayload);
    }

    if let Some((_before, after_hash)) = trimmed.split_once('#') {
        if after_hash.is_empty() {
            return Err(InviteError::MissingPayload);
        }
        for part in after_hash.split('&') {
            if let Some(val) = part.strip_prefix("payload=") {
                if val.is_empty() {
                    return Err(InviteError::MissingPayload);
                }
                return Ok(val);
            }
        }
        return Err(InviteError::MissingPayload);
    }

    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        if let Some((_before, query)) = trimmed.split_once('?') {
            for part in query.split('&') {
                if let Some(val) = part.strip_prefix("payload=") {
                    if val.is_empty() {
                        return Err(InviteError::MissingPayload);
                    }
                    return Ok(val);
                }
            }
        }
        return Err(InviteError::MissingPayload);
    }

    if trimmed.contains("payload=") {
        let stripped = trimmed.strip_prefix('?').unwrap_or(trimmed);
        for part in stripped.split('&') {
            if let Some(val) = part.strip_prefix("payload=") {
                if val.is_empty() {
                    return Err(InviteError::MissingPayload);
                }
                return Ok(val);
            }
        }
        return Err(InviteError::MissingPayload);
    }

    // Bare Base64URL string
    Ok(trimmed)
}

/// Unpacks an `InvitePayload` from a full URL, `#payload=...` fragment, query parameter,
/// or bare Base64URL string.
pub fn unpack_invite_fragment(fragment_or_url: &str) -> Result<InvitePayload, InviteError> {
    let raw_payload = extract_raw_payload(fragment_or_url)?;

    let decoded_bytes = URL_SAFE_NO_PAD
        .decode(raw_payload.as_bytes())
        .or_else(|_| URL_SAFE.decode(raw_payload.as_bytes()))
        .map_err(|e| InviteError::Base64(e.to_string()))?;

    let json_str = std::str::from_utf8(&decoded_bytes)
        .map_err(|e| InviteError::Serialization(e.to_string()))?;

    let payload: InvitePayload = from_json_str(json_str)?;

    Ok(payload)
}

/// Verifies the HMAC-SHA256 invite token in `payload` against `payload.invite_uuid` in constant time.
pub fn verify_invite_payload(
    host_secret: &[u8],
    payload: &InvitePayload,
) -> Result<(), InviteError> {
    verify_invite_token(host_secret, &payload.invite_uuid, &payload.token)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_offer() -> SdpOffer {
        SdpOffer {
            sdp: "v=0\r\no=- 12345 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n".to_string(),
        }
    }

    #[test]
    fn test_uuid_v4_generation_format() {
        let uuid = generate_uuid_v4().expect("generate uuid");
        assert_eq!(uuid.len(), 36);
        assert_eq!(&uuid[8..9], "-");
        assert_eq!(&uuid[13..14], "-");
        assert_eq!(&uuid[18..19], "-");
        assert_eq!(&uuid[23..24], "-");

        // Version 4 check
        assert_eq!(&uuid[14..15], "4");

        // RFC 4122 variant check: high nibble must be 8, 9, a, or b
        let variant_char = uuid.chars().nth(19).unwrap();
        assert!(
            ['8', '9', 'a', 'b'].contains(&variant_char),
            "Variant character was {variant_char}"
        );

        // Multiple calls generate unique UUIDs
        let uuid2 = generate_uuid_v4().expect("generate second uuid");
        assert_ne!(uuid, uuid2);
    }

    #[test]
    fn test_offline_invite_creation_and_verification() {
        let host_secret = b"super_secret_host_key_32_bytes!!";
        let host_public_key = [42u8; 32];
        let guest_priv = Some([99u8; 32]);

        // With custom UUID
        let payload = create_offline_invite(
            "do-chat-room-1",
            host_secret,
            host_public_key,
            dummy_offer(),
            Some("custom-uuid-001"),
            guest_priv,
        )
        .expect("create invite with custom uuid");

        assert_eq!(payload.do_id, "do-chat-room-1");
        assert_eq!(payload.invite_uuid, "custom-uuid-001");
        assert_eq!(payload.host_public_key, host_public_key);
        assert_eq!(payload.guest_private_key, guest_priv);
        assert_eq!(payload.sdp_offer, dummy_offer());

        // Constant-time HMAC token verification
        verify_invite_payload(host_secret, &payload).expect("verification should succeed");

        // Verification fails with wrong secret
        let err = verify_invite_payload(b"wrong_secret_key_value", &payload).unwrap_err();
        assert_eq!(
            err,
            InviteError::Protocol(ProtocolError::InvalidHmacSignature)
        );

        // Verification fails with empty secret
        let err = verify_invite_payload(b"", &payload).unwrap_err();
        assert_eq!(
            err,
            InviteError::Protocol(ProtocolError::InvalidKeyLength(0))
        );

        // With auto-generated UUID
        let payload_auto = create_offline_invite(
            "do-chat-room-2",
            host_secret,
            host_public_key,
            dummy_offer(),
            None,
            None,
        )
        .expect("create invite with generated uuid");

        assert_eq!(payload_auto.invite_uuid.len(), 36);
        assert_eq!(&payload_auto.invite_uuid[14..15], "4");
        verify_invite_payload(host_secret, &payload_auto).expect("auto uuid verification");
    }

    #[test]
    fn test_tampered_payload_verification_failure() {
        let host_secret = b"secret-key-1234567890";
        let mut payload = create_offline_invite(
            "do-room",
            host_secret,
            [1u8; 32],
            dummy_offer(),
            Some("valid-uuid"),
            None,
        )
        .expect("create invite");

        // Tamper with invite_uuid
        payload.invite_uuid = "tampered-uuid".to_string();
        let err = verify_invite_payload(host_secret, &payload).unwrap_err();
        assert_eq!(
            err,
            InviteError::Protocol(ProtocolError::InvalidHmacSignature)
        );

        // Reset and tamper with token
        payload.invite_uuid = "valid-uuid".to_string();
        payload.token = hex::encode([0u8; 32]);
        let err = verify_invite_payload(host_secret, &payload).unwrap_err();
        assert_eq!(
            err,
            InviteError::Protocol(ProtocolError::InvalidHmacSignature)
        );

        // Tamper with non-hex token
        payload.token = "not-a-valid-hex-string".to_string();
        let err = verify_invite_payload(host_secret, &payload).unwrap_err();
        assert_eq!(
            err,
            InviteError::Protocol(ProtocolError::InvalidHmacSignature)
        );
    }

    #[test]
    fn test_pack_and_unpack_fragment_roundtrip() {
        let host_secret = b"secret-room-token-hmac";
        let payload = create_offline_invite(
            "do-alpha",
            host_secret,
            [5u8; 32],
            dummy_offer(),
            Some("uuid-roundtrip-test"),
            Some([7u8; 32]),
        )
        .expect("create invite");

        let fragment = pack_invite_fragment(&payload).expect("pack fragment");
        assert!(fragment.starts_with("#payload="));

        // Unpack from `#payload=...`
        let unpacked = unpack_invite_fragment(&fragment).expect("unpack fragment");
        assert_eq!(payload, unpacked);
        verify_invite_payload(host_secret, &unpacked).expect("verify unpacked payload");

        // Unpack from bare `payload=...`
        let query_style = fragment.trim_start_matches('#');
        let unpacked_query = unpack_invite_fragment(query_style).expect("unpack query style");
        assert_eq!(payload, unpacked_query);

        // Unpack from bare Base64URL string
        let bare_base64 = fragment.trim_start_matches("#payload=");
        let unpacked_bare = unpack_invite_fragment(bare_base64).expect("unpack bare base64");
        assert_eq!(payload, unpacked_bare);
    }

    #[test]
    fn test_create_invite_url_and_unpack_variants() {
        let host_secret = b"my_host_secret_signing_key";
        let payload = create_offline_invite(
            "do-room-99",
            host_secret,
            [8u8; 32],
            dummy_offer(),
            Some("url-test-uuid"),
            None,
        )
        .expect("create invite");

        // Clean base URL
        let url =
            create_invite_url("https://zerochat.app/join", &payload).expect("create invite url");
        assert!(url.starts_with("https://zerochat.app/join#payload="));
        let unpacked = unpack_invite_fragment(&url).expect("unpack url");
        assert_eq!(payload, unpacked);

        // Base URL with existing fragment should be stripped and replaced
        let url_with_existing = create_invite_url("https://zerochat.app/join#stale-hash", &payload)
            .expect("replace existing fragment");
        assert!(url_with_existing.starts_with("https://zerochat.app/join#payload="));
        assert!(!url_with_existing.contains("stale-hash"));
        let unpacked2 = unpack_invite_fragment(&url_with_existing).expect("unpack replaced hash");
        assert_eq!(payload, unpacked2);

        // URL with path and query parameters preceding fragment
        let complex_url = format!(
            "https://zerochat.domain.com:8443/chat/app?theme=dark&lang=en{}",
            pack_invite_fragment(&payload).unwrap()
        );
        let unpacked_complex =
            unpack_invite_fragment(&complex_url).expect("unpack complex url with query");
        assert_eq!(payload, unpacked_complex);

        // Fragment with multiple parameters
        let raw_b64 = pack_invite_fragment(&payload)
            .unwrap()
            .replace("#payload=", "");
        let multi_param_fragment = format!("#theme=dark&payload={raw_b64}&debug=true");
        let unpacked_multi =
            unpack_invite_fragment(&multi_param_fragment).expect("unpack multi-param fragment");
        assert_eq!(payload, unpacked_multi);

        // Query param only in URL
        let url_query_only = format!("https://zerochat.domain.com/app?payload={raw_b64}");
        let unpacked_query_url =
            unpack_invite_fragment(&url_query_only).expect("unpack query param url");
        assert_eq!(payload, unpacked_query_url);
    }

    #[test]
    fn test_unpack_padded_base64() {
        let host_secret = b"padded_secret_test";
        let payload = create_offline_invite(
            "do-padded",
            host_secret,
            [12u8; 32],
            dummy_offer(),
            Some("uuid-pad-test"),
            None,
        )
        .expect("create invite");

        let json = to_json_string(&payload).unwrap();
        let padded_b64 = URL_SAFE.encode(json.as_bytes());
        let fragment = format!("#payload={padded_b64}");

        let unpacked = unpack_invite_fragment(&fragment).expect("unpack padded fragment");
        assert_eq!(payload, unpacked);
    }

    #[test]
    fn test_unpack_error_handling() {
        // Empty inputs
        assert_eq!(
            unpack_invite_fragment("").unwrap_err(),
            InviteError::MissingPayload
        );
        assert_eq!(
            unpack_invite_fragment("   ").unwrap_err(),
            InviteError::MissingPayload
        );

        // Missing payload parameter in URL or fragment
        assert_eq!(
            unpack_invite_fragment("#").unwrap_err(),
            InviteError::MissingPayload
        );
        assert_eq!(
            unpack_invite_fragment("#theme=dark").unwrap_err(),
            InviteError::MissingPayload
        );
        assert_eq!(
            unpack_invite_fragment("#payload=").unwrap_err(),
            InviteError::MissingPayload
        );
        assert_eq!(
            unpack_invite_fragment("https://example.com/join").unwrap_err(),
            InviteError::MissingPayload
        );
        assert_eq!(
            unpack_invite_fragment("https://example.com/join?foo=bar").unwrap_err(),
            InviteError::MissingPayload
        );

        // Invalid Base64
        let err = unpack_invite_fragment("#payload=@@@not_base64@@@").unwrap_err();
        match err {
            InviteError::Base64(_) => {}
            other => panic!("Expected Base64 error, got: {other:?}"),
        }

        // Valid Base64 but invalid JSON
        let non_json_b64 = URL_SAFE_NO_PAD.encode(b"this is not json content");
        let err = unpack_invite_fragment(&format!("#payload={non_json_b64}")).unwrap_err();
        match err {
            InviteError::Protocol(ProtocolError::SerializationError(_)) => {}
            other => panic!("Expected Serialization error in Protocol, got: {other:?}"),
        }
    }

    #[test]
    fn test_error_display() {
        assert_eq!(
            InviteError::Protocol(ProtocolError::InvalidHmacSignature).to_string(),
            "Protocol error: Invalid HMAC signature"
        );
        assert_eq!(
            InviteError::Base64("corrupt input".to_string()).to_string(),
            "Base64 decode error: corrupt input"
        );
        assert_eq!(
            InviteError::MissingPayload.to_string(),
            "Fragment is missing payload parameter"
        );
        assert_eq!(
            InviteError::InvalidFragment("bad fragment".to_string()).to_string(),
            "Invalid fragment: bad fragment"
        );
        assert_eq!(
            InviteError::Serialization("bad json".to_string()).to_string(),
            "Serialization error: bad json"
        );
    }
}
