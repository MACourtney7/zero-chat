//! Cryptographic payload sealing and authentication for zero_client.
//!
//! Provides per-message symmetric payload sealing and opening over WebRTC DataChannels
//! using X25519 shared secret derivation and ChaCha20-Poly1305 AEAD, satisfying REQ-005.2 and REQ-005.3.

use std::collections::HashMap;
use std::fmt;
use wasm_bindgen::prelude::*;
use zero_protocol::{
    crypto::{
        burner::BurnerKeybundle, chacha20poly1305, ed25519::verify_ed25519_signature, x25519,
    },
    envelope::{ChatMessage, ProtocolError, SealedFrame},
    from_json_str, to_json_string,
};

pub use zero_protocol::crypto::{KEY_LEN, NONCE_LEN, SIGNATURE_LEN, TAG_LEN};

/// Errors encountered during payload sealing, opening, key derivation, or deserialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoError {
    Protocol(ProtocolError),
    DecryptionFailed(String),
    MissingSealedFrame,
    InvalidKeyLength(usize),
    Utf8Error(String),
    Serialization(String),
}

impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Protocol(e) => write!(f, "Protocol error: {e}"),
            Self::DecryptionFailed(e) => write!(f, "Decryption/authentication failed: {e}"),
            Self::MissingSealedFrame => write!(f, "Missing sealed frame in chat message"),
            Self::InvalidKeyLength(len) => write!(f, "Invalid key length: {len}"),
            Self::Utf8Error(e) => write!(f, "UTF-8 decode error: {e}"),
            Self::Serialization(e) => write!(f, "Serialization error: {e}"),
        }
    }
}

impl std::error::Error for CryptoError {}

impl From<ProtocolError> for CryptoError {
    fn from(err: ProtocolError) -> Self {
        Self::Protocol(err)
    }
}

/// Cryptographic session manager holding the symmetric AEAD key with zeroization on drop.
#[derive(Clone, Debug)]
pub struct ChatSessionCrypto {
    pub shared_secret: [u8; KEY_LEN],
}

impl ChatSessionCrypto {
    /// Creates a session crypto instance directly from a 32-byte shared secret.
    pub fn from_shared_secret(shared_secret: [u8; KEY_LEN]) -> Self {
        Self { shared_secret }
    }

    /// Derives the 32-byte shared secret from our private key and the peer's public key using X25519.
    pub fn from_keypair_and_peer_public(
        our_private_key: &[u8; KEY_LEN],
        peer_public_key: &[u8; KEY_LEN],
    ) -> Self {
        let shared_secret = x25519::derive_shared_secret(our_private_key, peer_public_key);
        Self { shared_secret }
    }

    /// Returns a reference to the shared secret.
    pub fn shared_secret(&self) -> &[u8; KEY_LEN] {
        &self.shared_secret
    }

    /// Zeroizes the symmetric key material in-place using volatile writes.
    pub fn zeroize(&mut self) {
        for b in self.shared_secret.iter_mut() {
            unsafe {
                std::ptr::write_volatile(b, 0);
            }
        }
        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    }

    /// Encrypts and seals a plaintext message for transmission over RTCDataChannel (REQ-005.2).
    /// Generates a unique message UUID, assigns timestamp, seals plaintext with ChaCha20-Poly1305,
    /// and populates the `ChatMessage` struct with high-entropy `SealedFrame`.
    pub fn seal_message(
        &self,
        sender_id: &str,
        plaintext: &str,
    ) -> Result<ChatMessage, CryptoError> {
        let frame = chacha20poly1305::seal_payload(&self.shared_secret, plaintext.as_bytes())
            .map_err(CryptoError::Protocol)?;

        let message_uuid = generate_message_uuid()?;
        let timestamp_ms = current_timestamp_ms();

        Ok(ChatMessage {
            message_uuid,
            sender_id: sender_id.to_string(),
            timestamp_ms,
            body: String::new(), // Body remains empty; payload is inside sealed frame
            sealed: Some(frame),
        })
    }

    /// Authenticates and decrypts an incoming `ChatMessage` (REQ-005.3).
    /// Drops tampered or unauthenticated messages by returning `Err(CryptoError::DecryptionFailed)`.
    pub fn open_message(&self, message: &ChatMessage) -> Result<String, CryptoError> {
        let frame = message
            .sealed
            .as_ref()
            .ok_or(CryptoError::MissingSealedFrame)?;

        let decrypted_bytes = chacha20poly1305::open_payload(&self.shared_secret, frame)
            .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))?;

        String::from_utf8(decrypted_bytes).map_err(|e| CryptoError::Utf8Error(e.to_string()))
    }

    /// Directly seals arbitrary byte slices into a SealedFrame.
    pub fn seal_raw(&self, plaintext: &[u8]) -> Result<SealedFrame, CryptoError> {
        chacha20poly1305::seal_payload(&self.shared_secret, plaintext)
            .map_err(CryptoError::Protocol)
    }

    /// Directly authenticates and decrypts a SealedFrame into raw byte vector.
    pub fn open_raw(&self, frame: &SealedFrame) -> Result<Vec<u8>, CryptoError> {
        chacha20poly1305::open_payload(&self.shared_secret, frame)
            .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))
    }
}

impl Drop for ChatSessionCrypto {
    fn drop(&mut self) {
        self.zeroize();
    }
}

/// Computes X25519 Diffie-Hellman shared secret.
pub fn derive_session_key(
    our_private_key: &[u8; KEY_LEN],
    peer_public_key: &[u8; KEY_LEN],
) -> [u8; KEY_LEN] {
    x25519::derive_shared_secret(our_private_key, peer_public_key)
}

/// Convenience function to seal a chat payload with an explicit key.
pub fn seal_chat_payload(
    key: &[u8; KEY_LEN],
    sender_id: &str,
    plaintext: &str,
) -> Result<ChatMessage, CryptoError> {
    let crypto = ChatSessionCrypto::from_shared_secret(*key);
    crypto.seal_message(sender_id, plaintext)
}

/// Convenience function to open a chat payload with an explicit key.
pub fn open_chat_payload(
    key: &[u8; KEY_LEN],
    message: &ChatMessage,
) -> Result<String, CryptoError> {
    let crypto = ChatSessionCrypto::from_shared_secret(*key);
    crypto.open_message(message)
}

/// Generates a random v4 UUID for messages conforming to RFC 4122 using CSPRNG.
pub fn generate_message_uuid() -> Result<String, CryptoError> {
    zero_protocol::crypto::generate_uuid_v4().map_err(CryptoError::Protocol)
}

/// Returns current millisecond timestamp (WASM & native compatible).
pub fn current_timestamp_ms() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

/// Serializes a `ChatMessage` into a JSON string.
pub fn serialize_message(message: &ChatMessage) -> Result<String, CryptoError> {
    to_json_string(message).map_err(|e| CryptoError::Serialization(e.to_string()))
}

/// Deserializes a `ChatMessage` from a JSON string.
pub fn deserialize_message(json: &str) -> Result<ChatMessage, CryptoError> {
    from_json_str(json).map_err(|e| CryptoError::Serialization(e.to_string()))
}

/// Helper to construct a `JsValue` error message for WASM runtime while avoiding
/// unsupported runtime panics on native test targets.
#[inline]
fn js_error(msg: impl fmt::Display) -> JsValue {
    #[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
    {
        JsValue::from_str(&msg.to_string())
    }
    #[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
    {
        let _ = msg;
        JsValue::NULL
    }
}

/// Internal peer session tracking symmetric session crypto and optional peer Ed25519 public key.
struct PeerCryptoEntry {
    crypto: ChatSessionCrypto,
    ed25519_public: Option<[u8; KEY_LEN]>,
}

/// WASM cryptographic session manager exposing BurnerKeybundle, pairwise peer sessions,
/// authenticated frame sealing/opening, and blind dead-drop SDP encryption to JavaScript.
#[wasm_bindgen]
pub struct WasmCryptoSession {
    bundle: BurnerKeybundle,
    peer_sessions: HashMap<String, PeerCryptoEntry>,
}

#[wasm_bindgen]
impl WasmCryptoSession {
    /// Generates a fresh ephemeral BurnerKeybundle (X25519 + Ed25519) and empty peer sessions.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<WasmCryptoSession, JsValue> {
        let bundle = BurnerKeybundle::generate().map_err(js_error)?;
        Ok(WasmCryptoSession {
            bundle,
            peer_sessions: HashMap::new(),
        })
    }

    /// Returns the 32-byte X25519 public key bytes.
    pub fn x25519_public_key_bytes(&self) -> Vec<u8> {
        self.bundle.x25519_public_key().to_vec()
    }

    /// Returns the 32-byte Ed25519 public verifying key bytes.
    pub fn ed25519_public_key_bytes(&self) -> Vec<u8> {
        self.bundle.ed25519_public_key().to_vec()
    }

    /// Registers a peer session by deriving the pairwise X25519 shared secret
    /// and storing the optional Ed25519 public key for signature verification.
    pub fn register_peer(
        &mut self,
        peer_id: &str,
        peer_x25519_pub: &[u8],
        peer_ed25519_pub: Option<Vec<u8>>,
    ) -> Result<(), JsValue> {
        if peer_x25519_pub.len() != KEY_LEN {
            return Err(js_error(format!(
                "Invalid X25519 public key length: expected {}, got {}",
                KEY_LEN,
                peer_x25519_pub.len()
            )));
        }
        let mut x25519_arr = [0u8; KEY_LEN];
        x25519_arr.copy_from_slice(peer_x25519_pub);

        let ed25519_public = match peer_ed25519_pub {
            Some(bytes) => {
                if bytes.len() != KEY_LEN {
                    return Err(js_error(format!(
                        "Invalid Ed25519 public key length: expected {}, got {}",
                        KEY_LEN,
                        bytes.len()
                    )));
                }
                let mut ed_arr = [0u8; KEY_LEN];
                ed_arr.copy_from_slice(&bytes);
                Some(ed_arr)
            }
            None => None,
        };

        let crypto = ChatSessionCrypto::from_keypair_and_peer_public(
            self.bundle.x25519.private_key_bytes(),
            &x25519_arr,
        );

        self.peer_sessions.insert(
            peer_id.to_string(),
            PeerCryptoEntry {
                crypto,
                ed25519_public,
            },
        );
        Ok(())
    }

    /// Checks if a session crypto entry exists for `peer_id`.
    pub fn has_peer_session(&self, peer_id: &str) -> bool {
        self.peer_sessions.contains_key(peer_id)
    }

    /// Removes and drops the session crypto entry for `peer_id`, zeroizing its shared secret.
    pub fn remove_peer(&mut self, peer_id: &str) {
        self.peer_sessions.remove(peer_id);
    }

    /// Seals `plaintext` for `peer_id` using ChaCha20-Poly1305, signs (nonce || ciphertext)
    /// with self Ed25519 signing key, and returns the JSON-serialized SealedFrame.
    pub fn seal_for_peer(&self, peer_id: &str, plaintext: &[u8]) -> Result<String, JsValue> {
        let entry = self
            .peer_sessions
            .get(peer_id)
            .ok_or_else(|| js_error(format!("No peer session found for '{peer_id}'")))?;

        let mut frame = entry.crypto.seal_raw(plaintext).map_err(js_error)?;

        let mut to_sign = Vec::with_capacity(frame.nonce.len() + frame.ciphertext.len());
        to_sign.extend_from_slice(&frame.nonce);
        to_sign.extend_from_slice(&frame.ciphertext);

        let sig = self.bundle.ed25519.sign(&to_sign);
        frame.signature = Some(sig.to_vec());

        to_json_string(&frame).map_err(js_error)
    }

    /// Deserializes SealedFrame from JSON, verifies the Ed25519 signature if peer has
    /// ed25519_public and frame has signature, decrypts ciphertext, and returns plaintext bytes.
    pub fn open_from_peer(
        &self,
        peer_id: &str,
        sealed_frame_json: &str,
    ) -> Result<Vec<u8>, JsValue> {
        let frame: SealedFrame = from_json_str(sealed_frame_json)
            .map_err(|e| js_error(format!("Invalid SealedFrame JSON: {e}")))?;

        let entry = self
            .peer_sessions
            .get(peer_id)
            .ok_or_else(|| js_error(format!("No peer session found for '{peer_id}'")))?;

        if let (Some(ed_pub), Some(sig)) = (entry.ed25519_public.as_ref(), frame.signature.as_ref())
        {
            let sig_arr: [u8; SIGNATURE_LEN] = sig
                .as_slice()
                .try_into()
                .map_err(|_| js_error("Invalid Ed25519 signature length (expected 64 bytes)"))?;

            let mut to_verify = Vec::with_capacity(frame.nonce.len() + frame.ciphertext.len());
            to_verify.extend_from_slice(&frame.nonce);
            to_verify.extend_from_slice(&frame.ciphertext);

            verify_ed25519_signature(ed_pub, &to_verify, &sig_arr)
                .map_err(|e| js_error(format!("Ed25519 signature verification failed: {e}")))?;
        }

        entry.crypto.open_raw(&frame).map_err(js_error)
    }

    /// Registers the host peer session, seals the SDP plaintext into a signed SealedFrame,
    /// and returns the JSON string representation for `DropPayload.sealed_sdp_answer`.
    pub fn seal_drop_sdp(
        &mut self,
        host_peer_id: &str,
        host_x25519_pub: &[u8],
        host_ed25519_pub: Option<Vec<u8>>,
        sdp_plaintext: &str,
    ) -> Result<String, JsValue> {
        self.register_peer(host_peer_id, host_x25519_pub, host_ed25519_pub)?;
        self.seal_for_peer(host_peer_id, sdp_plaintext.as_bytes())
    }

    /// Registers the joiner peer session on the host, decrypts the sealed SDP JSON frame,
    /// verifies signature, and returns the plaintext SDP string.
    pub fn open_drop_sdp(
        &mut self,
        joiner_peer_id: &str,
        joiner_x25519_pub: &[u8],
        joiner_ed25519_pub: Option<Vec<u8>>,
        sealed_frame_json: &str,
    ) -> Result<String, JsValue> {
        self.register_peer(joiner_peer_id, joiner_x25519_pub, joiner_ed25519_pub)?;
        let plaintext_bytes = self.open_from_peer(joiner_peer_id, sealed_frame_json)?;
        String::from_utf8(plaintext_bytes)
            .map_err(|e| js_error(format!("Decrypted SDP is not valid UTF-8: {e}")))
    }

    /// Signs arbitrary bytes with self Ed25519 signing key.
    pub fn sign_bytes(&self, data: &[u8]) -> Vec<u8> {
        self.bundle.ed25519.sign(data).to_vec()
    }

    /// Verifies signature over `data` using `peer_id`'s registered Ed25519 public key.
    pub fn verify_peer_signature(
        &self,
        peer_id: &str,
        data: &[u8],
        sig: &[u8],
    ) -> Result<bool, JsValue> {
        let entry = self
            .peer_sessions
            .get(peer_id)
            .ok_or_else(|| js_error(format!("No peer session found for '{peer_id}'")))?;

        let ed_pub = match &entry.ed25519_public {
            Some(pubkey) => pubkey,
            None => return Ok(false),
        };

        let sig_arr: [u8; SIGNATURE_LEN] = match sig.try_into() {
            Ok(arr) => arr,
            Err(_) => return Ok(false),
        };

        match verify_ed25519_signature(ed_pub, data, &sig_arr) {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Computes HMAC-SHA256 hex string token for invite authentication.
    pub fn compute_hmac_hex(secret: &[u8], invite_uuid: &str) -> Result<String, JsValue> {
        zero_protocol::crypto::hmac_sha256::compute_invite_token(secret, invite_uuid)
            .map(|token| token.hmac_hex)
            .map_err(js_error)
    }

    /// Clears all peer sessions and zeroizes key material in memory.
    pub fn burn_all(&mut self) {
        self.peer_sessions.clear();
        self.bundle.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_x25519_key_exchange_symmetry() {
        let alice_kp = x25519::X25519Keypair::generate().expect("alice keypair");
        let bob_kp = x25519::X25519Keypair::generate().expect("bob keypair");

        let alice_crypto = ChatSessionCrypto::from_keypair_and_peer_public(
            alice_kp.private_key_bytes(),
            bob_kp.public_key_bytes(),
        );

        let bob_crypto = ChatSessionCrypto::from_keypair_and_peer_public(
            bob_kp.private_key_bytes(),
            alice_kp.public_key_bytes(),
        );

        assert_eq!(
            alice_crypto.shared_secret(),
            bob_crypto.shared_secret(),
            "Diffie-Hellman shared secret must be symmetric"
        );

        let direct_alice =
            derive_session_key(alice_kp.private_key_bytes(), bob_kp.public_key_bytes());
        let direct_bob =
            derive_session_key(bob_kp.private_key_bytes(), alice_kp.public_key_bytes());
        assert_eq!(direct_alice, direct_bob);
        assert_eq!(alice_crypto.shared_secret(), &direct_alice);
    }

    #[test]
    fn test_seal_and_open_roundtrip() {
        let secret = [0x3a; KEY_LEN];
        let alice_crypto = ChatSessionCrypto::from_shared_secret(secret);
        let bob_crypto = ChatSessionCrypto::from_shared_secret(secret);

        let plaintext = "Hello peer! This is an ephemeral sealed chat message.";
        let chat_msg = alice_crypto
            .seal_message("alice-uuid-1", plaintext)
            .expect("seal message");

        assert_eq!(chat_msg.sender_id, "alice-uuid-1");
        assert!(chat_msg.body.is_empty(), "Plaintext body must remain empty");
        assert!(chat_msg.sealed.is_some(), "Sealed frame must be populated");
        assert_eq!(chat_msg.message_uuid.len(), 36);
        assert!(chat_msg.timestamp_ms > 0);

        let decrypted = bob_crypto.open_message(&chat_msg).expect("open message");
        assert_eq!(decrypted, plaintext);

        // Test convenience functions
        let conv_msg =
            seal_chat_payload(&secret, "alice-conv", plaintext).expect("convenience seal");
        let conv_decrypted = open_chat_payload(&secret, &conv_msg).expect("convenience open");
        assert_eq!(conv_decrypted, plaintext);
    }

    #[test]
    fn test_packet_structure_entropy_and_lengths() {
        let secret = [0x88; KEY_LEN];
        let crypto = ChatSessionCrypto::from_shared_secret(secret);

        let plaintext =
            "High entropy ChaCha20-Poly1305 ciphertext structure validation test string.";
        let chat_msg = crypto
            .seal_message("sender-test", plaintext)
            .expect("seal message");
        let frame = chat_msg.sealed.as_ref().expect("sealed frame exists");

        // Length checks: nonce = 12 bytes, ciphertext = plaintext.len() + 16 (Poly1305 tag)
        assert_eq!(frame.nonce.len(), NONCE_LEN);
        assert_eq!(frame.ciphertext.len(), plaintext.len() + TAG_LEN);

        // Ciphertext should not match plaintext
        assert_ne!(frame.ciphertext.as_slice(), plaintext.as_bytes());

        // Entropy check: ensure ciphertext has a diverse byte distribution
        let mut distinct_bytes = std::collections::HashSet::new();
        for &byte in &frame.ciphertext {
            distinct_bytes.insert(byte);
        }
        // For ~90 bytes of ciphertext, distinct bytes should easily exceed 30
        assert!(
            distinct_bytes.len() >= 30,
            "Ciphertext distinct byte diversity was unexpectedly low: {}",
            distinct_bytes.len()
        );
    }

    #[test]
    fn test_tampered_ciphertext_rejection() {
        let secret = [0x55; KEY_LEN];
        let crypto = ChatSessionCrypto::from_shared_secret(secret);

        let mut chat_msg = crypto
            .seal_message("alice", "Authentic payload to be tampered")
            .expect("seal");

        let frame = chat_msg.sealed.as_mut().expect("sealed frame");
        // Flip a bit in the ciphertext payload
        frame.ciphertext[0] ^= 0x01;

        let result = crypto.open_message(&chat_msg);
        match result {
            Err(CryptoError::DecryptionFailed(_)) => {}
            other => {
                panic!("Expected DecryptionFailed error on tampered ciphertext, got {other:?}")
            }
        }
    }

    #[test]
    fn test_tampered_nonce_rejection() {
        let secret = [0x66; KEY_LEN];
        let crypto = ChatSessionCrypto::from_shared_secret(secret);

        let mut chat_msg = crypto
            .seal_message("alice", "Authentic payload with tampered nonce")
            .expect("seal");

        let frame = chat_msg.sealed.as_mut().expect("sealed frame");
        // Flip a bit in the nonce
        frame.nonce[0] ^= 0x01;

        let result = crypto.open_message(&chat_msg);
        match result {
            Err(CryptoError::DecryptionFailed(_)) => {}
            other => panic!("Expected DecryptionFailed error on tampered nonce, got {other:?}"),
        }
    }

    #[test]
    fn test_wrong_key_rejection() {
        let key_alice = [0x11; KEY_LEN];
        let key_eavesdropper = [0x22; KEY_LEN];

        let alice_crypto = ChatSessionCrypto::from_shared_secret(key_alice);
        let intruder_crypto = ChatSessionCrypto::from_shared_secret(key_eavesdropper);

        let chat_msg = alice_crypto
            .seal_message("alice", "Top secret confidential communication")
            .expect("seal");

        let result = intruder_crypto.open_message(&chat_msg);
        match result {
            Err(CryptoError::DecryptionFailed(_)) => {}
            other => panic!("Expected DecryptionFailed error with wrong key, got {other:?}"),
        }
    }

    #[test]
    fn test_empty_message_handling() {
        let secret = [0xfe; KEY_LEN];
        let crypto = ChatSessionCrypto::from_shared_secret(secret);

        let chat_msg = crypto.seal_message("alice", "").expect("seal empty");
        let frame = chat_msg.sealed.as_ref().expect("sealed frame");

        assert_eq!(frame.ciphertext.len(), TAG_LEN); // Exactly 16-byte Poly1305 MAC tag
        let decrypted = crypto.open_message(&chat_msg).expect("open empty");
        assert_eq!(decrypted, "");
    }

    #[test]
    fn test_zeroize_on_drop() {
        let mut crypto = ChatSessionCrypto::from_shared_secret([0x77; KEY_LEN]);
        assert_eq!(crypto.shared_secret(), &[0x77; KEY_LEN]);

        // Explicit in-place zeroize wipes the key bytes
        crypto.zeroize();
        assert_eq!(crypto.shared_secret(), &[0u8; KEY_LEN]);

        // Dropping executes without panic
        drop(crypto);
    }

    #[test]
    fn test_missing_sealed_frame_rejection() {
        let secret = [0x99; KEY_LEN];
        let crypto = ChatSessionCrypto::from_shared_secret(secret);

        let chat_msg = ChatMessage {
            message_uuid: generate_message_uuid().unwrap(),
            sender_id: "alice".to_string(),
            timestamp_ms: current_timestamp_ms(),
            body: "unsealed message body".to_string(),
            sealed: None,
        };

        let result = crypto.open_message(&chat_msg);
        assert_eq!(result, Err(CryptoError::MissingSealedFrame));
    }

    #[test]
    fn test_raw_seal_and_open() {
        let secret = [0xab; KEY_LEN];
        let crypto = ChatSessionCrypto::from_shared_secret(secret);

        let raw_data = vec![0x00, 0xff, 0x12, 0x34, 0x00, 0x99];
        let frame = crypto.seal_raw(&raw_data).expect("seal raw");
        assert_eq!(frame.ciphertext.len(), raw_data.len() + TAG_LEN);

        let opened = crypto.open_raw(&frame).expect("open raw");
        assert_eq!(opened, raw_data);
    }

    #[test]
    fn test_non_utf8_decryption_returns_utf8_error() {
        let secret = [0xcd; KEY_LEN];
        let crypto = ChatSessionCrypto::from_shared_secret(secret);

        // Invalid UTF-8 sequence
        let invalid_utf8 = vec![0x80, 0x81, 0xff];
        let frame = crypto.seal_raw(&invalid_utf8).expect("seal invalid utf8");

        let msg = ChatMessage {
            message_uuid: generate_message_uuid().unwrap(),
            sender_id: "alice".to_string(),
            timestamp_ms: current_timestamp_ms(),
            body: String::new(),
            sealed: Some(frame),
        };

        let result = crypto.open_message(&msg);
        match result {
            Err(CryptoError::Utf8Error(_)) => {}
            other => panic!("Expected Utf8Error, got {other:?}"),
        }
    }

    #[test]
    fn test_message_serialization_roundtrip() {
        let secret = [0x44; KEY_LEN];
        let crypto = ChatSessionCrypto::from_shared_secret(secret);

        let msg = crypto
            .seal_message("bob", "Testing JSON serialization")
            .unwrap();
        let json = serialize_message(&msg).expect("serialize");
        let decoded = deserialize_message(&json).expect("deserialize");
        assert_eq!(msg, decoded);

        let opened = crypto.open_message(&decoded).expect("open decoded");
        assert_eq!(opened, "Testing JSON serialization");
    }

    #[test]
    fn test_message_uuid_format() {
        let uuid1 = generate_message_uuid().expect("generate uuid");
        assert_eq!(uuid1.len(), 36);
        assert_eq!(&uuid1[8..9], "-");
        assert_eq!(&uuid1[13..14], "-");
        assert_eq!(&uuid1[18..19], "-");
        assert_eq!(&uuid1[23..24], "-");

        // Check RFC 4122 v4: 13th index (high nibble of byte 6) is '4'
        assert_eq!(&uuid1[14..15], "4");

        // Check RFC 4122 variant: 19th index (high nibble of byte 8) is '8', '9', 'a', or 'b'
        let variant_char = uuid1.chars().nth(19).unwrap();
        assert!(
            ['8', '9', 'a', 'b'].contains(&variant_char),
            "Variant character was {variant_char}"
        );

        let uuid2 = generate_message_uuid().expect("generate second uuid");
        assert_ne!(uuid1, uuid2);
    }

    #[test]
    fn test_error_display_formatting() {
        assert_eq!(
            CryptoError::Protocol(ProtocolError::ExpiredTtl).to_string(),
            "Protocol error: Expired TTL"
        );
        assert_eq!(
            CryptoError::DecryptionFailed("bad mac".to_string()).to_string(),
            "Decryption/authentication failed: bad mac"
        );
        assert_eq!(
            CryptoError::MissingSealedFrame.to_string(),
            "Missing sealed frame in chat message"
        );
        assert_eq!(
            CryptoError::InvalidKeyLength(16).to_string(),
            "Invalid key length: 16"
        );
        assert_eq!(
            CryptoError::Utf8Error("invalid utf8".to_string()).to_string(),
            "UTF-8 decode error: invalid utf8"
        );
        assert_eq!(
            CryptoError::Serialization("json err".to_string()).to_string(),
            "Serialization error: json err"
        );
    }

    #[test]
    fn test_wasm_crypto_session_keys_and_registration() {
        let mut alice = WasmCryptoSession::new().expect("alice session");
        let bob = WasmCryptoSession::new().expect("bob session");

        let alice_x = alice.x25519_public_key_bytes();
        let alice_ed = alice.ed25519_public_key_bytes();
        let bob_x = bob.x25519_public_key_bytes();
        let bob_ed = bob.ed25519_public_key_bytes();

        assert_eq!(alice_x.len(), KEY_LEN);
        assert_eq!(alice_ed.len(), KEY_LEN);
        assert_eq!(bob_x.len(), KEY_LEN);
        assert_eq!(bob_ed.len(), KEY_LEN);

        assert!(!alice.has_peer_session("bob"));
        alice
            .register_peer("bob", &bob_x, Some(bob_ed.clone()))
            .expect("register bob");
        assert!(alice.has_peer_session("bob"));

        // Reject invalid key lengths
        let invalid_key = vec![0u8; 16];
        assert!(alice.register_peer("bad_peer", &invalid_key, None).is_err());
        assert!(alice
            .register_peer("bad_peer", &bob_x, Some(invalid_key))
            .is_err());

        // Remove peer
        alice.remove_peer("bob");
        assert!(!alice.has_peer_session("bob"));
    }

    #[test]
    fn test_wasm_crypto_session_sealing_and_opening_roundtrip() {
        let mut alice = WasmCryptoSession::new().expect("alice session");
        let mut bob = WasmCryptoSession::new().expect("bob session");

        alice
            .register_peer(
                "bob",
                &bob.x25519_public_key_bytes(),
                Some(bob.ed25519_public_key_bytes()),
            )
            .expect("register bob on alice");

        bob.register_peer(
            "alice",
            &alice.x25519_public_key_bytes(),
            Some(alice.ed25519_public_key_bytes()),
        )
        .expect("register alice on bob");

        let msg = b"Encrypted and Ed25519-signed WASM payload";
        let sealed_json = alice.seal_for_peer("bob", msg).expect("seal for bob");

        let decrypted = bob
            .open_from_peer("alice", &sealed_json)
            .expect("open from alice");
        assert_eq!(decrypted, msg);

        // Tamper with ciphertext
        let mut frame: SealedFrame = from_json_str(&sealed_json).unwrap();
        frame.ciphertext[0] ^= 0xff;
        let tampered_json = to_json_string(&frame).unwrap();
        assert!(bob.open_from_peer("alice", &tampered_json).is_err());

        // Tamper with signature
        let mut frame_bad_sig: SealedFrame = from_json_str(&sealed_json).unwrap();
        if let Some(ref mut sig) = frame_bad_sig.signature {
            sig[0] ^= 0x01;
        }
        let bad_sig_json = to_json_string(&frame_bad_sig).unwrap();
        assert!(bob.open_from_peer("alice", &bad_sig_json).is_err());
    }

    #[test]
    fn test_wasm_crypto_session_seal_and_open_drop_sdp() {
        let mut host = WasmCryptoSession::new().expect("host session");
        let mut joiner = WasmCryptoSession::new().expect("joiner session");

        let host_x = host.x25519_public_key_bytes();
        let host_ed = host.ed25519_public_key_bytes();
        let joiner_x = joiner.x25519_public_key_bytes();
        let joiner_ed = joiner.ed25519_public_key_bytes();

        let sdp_content =
            "v=0\r\no=- 123456 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\na=crypto-bridge\r\n";

        // Joiner seals SDP for host
        let sealed_sdp_json = joiner
            .seal_drop_sdp("host-id", &host_x, Some(host_ed), sdp_content)
            .expect("joiner seal drop sdp");

        // Host opens SDP from joiner
        let opened_sdp = host
            .open_drop_sdp("joiner-id", &joiner_x, Some(joiner_ed), &sealed_sdp_json)
            .expect("host open drop sdp");

        assert_eq!(opened_sdp, sdp_content);

        // Tampered SDP fails
        let mut frame: SealedFrame = from_json_str(&sealed_sdp_json).unwrap();
        frame.ciphertext[2] ^= 0xaa;
        let tampered_json = to_json_string(&frame).unwrap();
        assert!(host
            .open_drop_sdp("joiner-id", &joiner_x, None, &tampered_json)
            .is_err());
    }

    #[test]
    fn test_wasm_crypto_session_sign_and_verify() {
        let alice = WasmCryptoSession::new().expect("alice session");
        let mut bob = WasmCryptoSession::new().expect("bob session");

        bob.register_peer(
            "alice",
            &alice.x25519_public_key_bytes(),
            Some(alice.ed25519_public_key_bytes()),
        )
        .expect("register alice");

        let data = b"Arbitrary binary protocol message to authenticate";
        let sig = alice.sign_bytes(data);
        assert_eq!(sig.len(), SIGNATURE_LEN);

        let valid = bob
            .verify_peer_signature("alice", data, &sig)
            .expect("verify signature");
        assert!(valid);

        let invalid_data = b"Tampered data";
        let invalid = bob
            .verify_peer_signature("alice", invalid_data, &sig)
            .expect("verify invalid data");
        assert!(!invalid);

        let invalid_sig_len = vec![0u8; 32];
        let bad_len_result = bob
            .verify_peer_signature("alice", data, &invalid_sig_len)
            .expect("bad sig len returns false");
        assert!(!bad_len_result);
    }

    #[test]
    fn test_wasm_crypto_session_compute_hmac_hex() {
        let secret = b"super_secret_key_32_bytes_value!";
        let uuid = "9b1deb4d-3b7d-4bad-9bdd-2b0d7b3dcb6d";
        let token = WasmCryptoSession::compute_hmac_hex(secret, uuid).expect("compute hmac");
        assert_eq!(token.len(), 64);

        // Verify against zero_protocol directly
        let expected =
            zero_protocol::crypto::hmac_sha256::compute_invite_token(secret, uuid).unwrap();
        assert_eq!(token, expected.hmac_hex);
    }

    #[test]
    fn test_wasm_crypto_session_burn_all() {
        let mut alice = WasmCryptoSession::new().expect("alice session");
        let bob = WasmCryptoSession::new().expect("bob session");

        alice
            .register_peer("bob", &bob.x25519_public_key_bytes(), None)
            .expect("register bob");
        assert!(alice.has_peer_session("bob"));

        alice.burn_all();
        assert!(!alice.has_peer_session("bob"));
    }
}
