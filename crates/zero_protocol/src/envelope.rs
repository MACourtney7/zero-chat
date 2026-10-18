use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolError {
    SerializationError(String),
    InvalidHmacSignature,
    EncryptionError(String),
    DecryptionError(String),
    InvalidKeyLength(usize),
    InvalidNonceLength(usize),
    RandomGenerationError(String),
    ExpiredTtl,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProtocolError::SerializationError(msg) => write!(f, "Serialization error: {msg}"),
            ProtocolError::InvalidHmacSignature => write!(f, "Invalid HMAC signature"),
            ProtocolError::EncryptionError(msg) => write!(f, "Encryption error: {msg}"),
            ProtocolError::DecryptionError(msg) => write!(f, "Decryption error: {msg}"),
            ProtocolError::InvalidKeyLength(len) => write!(f, "Invalid key length: {len}"),
            ProtocolError::InvalidNonceLength(len) => write!(f, "Invalid nonce length: {len}"),
            ProtocolError::RandomGenerationError(msg) => {
                write!(f, "Random generation error: {msg}")
            }
            ProtocolError::ExpiredTtl => write!(f, "Expired TTL"),
        }
    }
}

impl std::error::Error for ProtocolError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SdpOffer {
    pub sdp: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SdpAnswer {
    pub sdp: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InviteToken {
    pub invite_uuid: String,
    pub hmac_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvitePayload {
    pub do_id: String,
    pub invite_uuid: String,
    pub token: String,
    pub sdp_offer: SdpOffer,
    pub host_public_key: [u8; 32],
    pub guest_private_key: Option<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DropPayload {
    pub invite_uuid: String,
    pub token: String,
    pub sdp_answer: SdpAnswer,
    pub joiner_public_key: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedFrame {
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub message_uuid: String,
    pub sender_id: String,
    pub timestamp_ms: u64,
    pub body: String,
    pub sealed: Option<SealedFrame>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MeshRelaySignal {
    PeerJoined {
        peer_id: String,
        public_key: [u8; 32],
    },
    PeerLeft {
        peer_id: String,
    },
    PeerLeftAndBurn {
        peer_id: String,
    },
    GlobalBurnSession {
        initiator_peer_id: String,
    },
    RelaySdpOffer {
        from_peer_id: String,
        to_peer_id: String,
        sealed_offer: SealedFrame,
    },
    RelaySdpAnswer {
        from_peer_id: String,
        to_peer_id: String,
        sealed_answer: SealedFrame,
    },
    BroadcastChat {
        message_uuid: String,
        origin_peer_id: String,
        ttl: u8,
        sealed_message: SealedFrame,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeshRelayMessage {
    pub frame_id: String,
    pub ttl: u8,
    pub signal: MeshRelaySignal,
}

impl MeshRelayMessage {
    pub fn new(frame_id: impl Into<String>, ttl: u8, signal: MeshRelaySignal) -> Self {
        Self {
            frame_id: frame_id.into(),
            ttl,
            signal,
        }
    }

    /// Decrements the TTL by 1. Returns `true` if TTL remains > 0 (can be forwarded),
    /// or `false` if the TTL has expired (must be dropped).
    pub fn decrement_ttl(&mut self) -> bool {
        if self.ttl > 0 {
            self.ttl -= 1;
        }
        self.ttl > 0
    }
}

pub fn to_json_string<T: Serialize>(value: &T) -> Result<String, ProtocolError> {
    serde_json::to_string(value).map_err(|e| ProtocolError::SerializationError(e.to_string()))
}

pub fn from_json_str<'a, T: Deserialize<'a>>(json: &'a str) -> Result<T, ProtocolError> {
    serde_json::from_str(json).map_err(|e| ProtocolError::SerializationError(e.to_string()))
}

pub fn to_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, ProtocolError> {
    serde_json::to_vec(value).map_err(|e| ProtocolError::SerializationError(e.to_string()))
}

pub fn from_json_bytes<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, ProtocolError> {
    serde_json::from_slice(bytes).map_err(|e| ProtocolError::SerializationError(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invite_payload_roundtrip() {
        let payload = InvitePayload {
            do_id: "do-123".to_string(),
            invite_uuid: "uuid-456".to_string(),
            token: "hmac-token".to_string(),
            sdp_offer: SdpOffer {
                sdp: "v=0\r\no=- 0 0 IN IP4 127.0.0.1".to_string(),
            },
            host_public_key: [7u8; 32],
            guest_private_key: Some([9u8; 32]),
        };

        let json = to_json_string(&payload).expect("serialize to string");
        let decoded: InvitePayload = from_json_str(&json).expect("deserialize from str");
        assert_eq!(payload, decoded);

        let bytes = to_json_bytes(&payload).expect("serialize to bytes");
        let decoded_bytes: InvitePayload = from_json_bytes(&bytes).expect("deserialize from bytes");
        assert_eq!(payload, decoded_bytes);
    }

    #[test]
    fn test_drop_payload_roundtrip() {
        let payload = DropPayload {
            invite_uuid: "uuid-456".to_string(),
            token: "hmac-token".to_string(),
            sdp_answer: SdpAnswer {
                sdp: "v=0\r\na=answer".to_string(),
            },
            joiner_public_key: [11u8; 32],
        };

        let json = to_json_string(&payload).expect("serialize to string");
        let decoded: DropPayload = from_json_str(&json).expect("deserialize from str");
        assert_eq!(payload, decoded);

        let bytes = to_json_bytes(&payload).expect("serialize to bytes");
        let decoded_bytes: DropPayload = from_json_bytes(&bytes).expect("deserialize from bytes");
        assert_eq!(payload, decoded_bytes);
    }

    #[test]
    fn test_chat_message_roundtrip() {
        let msg = ChatMessage {
            message_uuid: "msg-001".to_string(),
            sender_id: "peer-a".to_string(),
            timestamp_ms: 1700000000000,
            body: "Hello zero_chat!".to_string(),
            sealed: Some(SealedFrame {
                nonce: [1u8; 12],
                ciphertext: vec![10, 20, 30, 40],
            }),
        };

        let json = to_json_string(&msg).expect("serialize to string");
        let decoded: ChatMessage = from_json_str(&json).expect("deserialize from str");
        assert_eq!(msg, decoded);

        let bytes = to_json_bytes(&msg).expect("serialize to bytes");
        let decoded_bytes: ChatMessage = from_json_bytes(&bytes).expect("deserialize from bytes");
        assert_eq!(msg, decoded_bytes);
    }

    #[test]
    fn test_mesh_relay_message_roundtrip() {
        let relay = MeshRelayMessage {
            frame_id: "frame-99".to_string(),
            ttl: 3,
            signal: MeshRelaySignal::BroadcastChat {
                message_uuid: "msg-001".to_string(),
                origin_peer_id: "peer-a".to_string(),
                ttl: 3,
                sealed_message: SealedFrame {
                    nonce: [2u8; 12],
                    ciphertext: vec![99, 88, 77],
                },
            },
        };

        let json = to_json_string(&relay).expect("serialize to string");
        let decoded: MeshRelayMessage = from_json_str(&json).expect("deserialize from str");
        assert_eq!(relay, decoded);

        let bytes = to_json_bytes(&relay).expect("serialize to bytes");
        let decoded_bytes: MeshRelayMessage =
            from_json_bytes(&bytes).expect("deserialize from bytes");
        assert_eq!(relay, decoded_bytes);
    }

    #[test]
    fn test_mesh_relay_peer_left_roundtrip() {
        let relay = MeshRelayMessage::new(
            "frame-100",
            3,
            MeshRelaySignal::PeerLeft {
                peer_id: "peer-b".to_string(),
            },
        );
        let json = to_json_string(&relay).expect("serialize to string");
        let decoded: MeshRelayMessage = from_json_str(&json).expect("deserialize from str");
        assert_eq!(relay, decoded);

        let bytes = to_json_bytes(&relay).expect("serialize to bytes");
        let decoded_bytes: MeshRelayMessage =
            from_json_bytes(&bytes).expect("deserialize from bytes");
        assert_eq!(relay, decoded_bytes);
    }

    #[test]
    fn test_mesh_relay_peer_left_and_burn_roundtrip() {
        let mut relay = MeshRelayMessage::new(
            "frame-burn-1",
            3,
            MeshRelaySignal::PeerLeftAndBurn {
                peer_id: "peer-b".to_string(),
            },
        );
        let json = to_json_string(&relay).expect("serialize to string");
        let decoded: MeshRelayMessage = from_json_str(&json).expect("deserialize from str");
        assert_eq!(relay, decoded);

        let bytes = to_json_bytes(&relay).expect("serialize to bytes");
        let decoded_bytes: MeshRelayMessage =
            from_json_bytes(&bytes).expect("deserialize from bytes");
        assert_eq!(relay, decoded_bytes);

        assert!(relay.decrement_ttl());
        assert_eq!(relay.ttl, 2);
    }

    #[test]
    fn test_mesh_relay_global_burn_session_roundtrip() {
        let mut relay = MeshRelayMessage::new(
            "frame-burn-global-1",
            3,
            MeshRelaySignal::GlobalBurnSession {
                initiator_peer_id: "peer-a".to_string(),
            },
        );
        let json = to_json_string(&relay).expect("serialize to string");
        let decoded: MeshRelayMessage = from_json_str(&json).expect("deserialize from str");
        assert_eq!(relay, decoded);

        let bytes = to_json_bytes(&relay).expect("serialize to bytes");
        let decoded_bytes: MeshRelayMessage =
            from_json_bytes(&bytes).expect("deserialize from bytes");
        assert_eq!(relay, decoded_bytes);

        assert!(relay.decrement_ttl());
        assert_eq!(relay.ttl, 2);
    }

    #[test]
    fn test_mesh_relay_decrement_ttl() {
        let mut relay = MeshRelayMessage::new(
            "frame-101",
            2,
            MeshRelaySignal::PeerLeft {
                peer_id: "peer-c".to_string(),
            },
        );
        assert_eq!(relay.ttl, 2);
        assert!(relay.decrement_ttl());
        assert_eq!(relay.ttl, 1);
        assert!(!relay.decrement_ttl());
        assert_eq!(relay.ttl, 0);
        assert!(!relay.decrement_ttl());
        assert_eq!(relay.ttl, 0);
    }
}
