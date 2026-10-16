//! Target-independent cryptographic primitives for zero_chat:
//! - HMAC-SHA256 invite token generation and constant-time verification
//! - X25519 ephemeral keypair generation and Diffie-Hellman key agreement with zeroization on drop
//! - ChaCha20-Poly1305 AEAD payload sealing and opening

use crate::envelope::{InviteToken, ProtocolError, SealedFrame};

/// Standard 256-bit (32-byte) key length for X25519 and ChaCha20-Poly1305.
pub const KEY_LEN: usize = 32;

/// Standard 96-bit (12-byte) nonce length for ChaCha20-Poly1305.
pub const NONCE_LEN: usize = 12;

/// Standard 128-bit (16-byte) Poly1305 authentication tag length.
pub const TAG_LEN: usize = 16;

/// HMAC-SHA256 invite token generation and constant-time verification.
pub mod hmac_sha256 {
    use super::*;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    /// Computes a 32-byte `HMAC-SHA256(secret, message)`.
    /// Returns `Err(ProtocolError::InvalidKeyLength(0))` if `secret` is empty.
    pub fn compute_hmac_sha256(secret: &[u8], message: &[u8]) -> Result<[u8; 32], ProtocolError> {
        if secret.is_empty() {
            return Err(ProtocolError::InvalidKeyLength(0));
        }

        let mut mac = HmacSha256::new_from_slice(secret)
            .map_err(|_| ProtocolError::InvalidKeyLength(secret.len()))?;
        mac.update(message);

        let result = mac.finalize().into_bytes();
        let mut out = [0u8; 32];
        out.copy_from_slice(&result);
        Ok(out)
    }

    /// Verifies `HMAC-SHA256(secret, message)` against `expected_tag` in constant time.
    /// Returns `Err(ProtocolError::InvalidKeyLength(0))` if `secret` is empty, or
    /// `Err(ProtocolError::InvalidHmacSignature)` if verification fails.
    pub fn verify_hmac_sha256(
        secret: &[u8],
        message: &[u8],
        expected_tag: &[u8],
    ) -> Result<(), ProtocolError> {
        if secret.is_empty() {
            return Err(ProtocolError::InvalidKeyLength(0));
        }

        let mut mac = HmacSha256::new_from_slice(secret)
            .map_err(|_| ProtocolError::InvalidKeyLength(secret.len()))?;
        mac.update(message);

        mac.verify_slice(expected_tag)
            .map_err(|_| ProtocolError::InvalidHmacSignature)
    }

    /// Computes a hex-encoded `InviteToken` over `invite_uuid` using `host_secret`.
    pub fn compute_invite_token(
        host_secret: &[u8],
        invite_uuid: &str,
    ) -> Result<InviteToken, ProtocolError> {
        let mac = compute_hmac_sha256(host_secret, invite_uuid.as_bytes())?;
        Ok(InviteToken {
            invite_uuid: invite_uuid.to_string(),
            hmac_hex: hex::encode(mac),
        })
    }

    /// Verifies a hex-encoded `hmac_hex` invite token over `invite_uuid` in constant time.
    pub fn verify_invite_token(
        host_secret: &[u8],
        invite_uuid: &str,
        hmac_hex: &str,
    ) -> Result<(), ProtocolError> {
        if host_secret.is_empty() {
            return Err(ProtocolError::InvalidKeyLength(0));
        }
        let decoded = hex::decode(hmac_hex).map_err(|_| ProtocolError::InvalidHmacSignature)?;
        verify_hmac_sha256(host_secret, invite_uuid.as_bytes(), &decoded)
    }
}

/// X25519 ephemeral keypair generation and Diffie-Hellman shared secret derivation.
pub mod x25519 {
    use super::*;
    use x25519_dalek::{PublicKey, StaticSecret};
    use zeroize::{Zeroize, ZeroizeOnDrop};

    /// Ephemeral X25519 keypair whose private key material is automatically zeroized on drop.
    #[derive(Clone, Debug, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
    pub struct X25519Keypair {
        pub private_key: [u8; KEY_LEN],
        pub public_key: [u8; KEY_LEN],
    }

    impl X25519Keypair {
        /// Generates a fresh random X25519 keypair using `getrandom`.
        pub fn generate() -> Result<Self, ProtocolError> {
            let mut seed = [0u8; KEY_LEN];
            getrandom::getrandom(&mut seed)
                .map_err(|e| ProtocolError::RandomGenerationError(e.to_string()))?;
            let keypair = Self::from_private_key(seed);
            seed.zeroize();
            Ok(keypair)
        }

        /// Deterministically constructs an `X25519Keypair` from a 32-byte private key seed.
        pub fn from_private_key(private_key: [u8; KEY_LEN]) -> Self {
            let secret = StaticSecret::from(private_key);
            let public = PublicKey::from(&secret);
            Self {
                private_key: secret.to_bytes(),
                public_key: public.to_bytes(),
            }
        }

        /// Returns a reference to the 32-byte private key.
        pub fn private_key_bytes(&self) -> &[u8; KEY_LEN] {
            &self.private_key
        }

        /// Returns a reference to the 32-byte public key.
        pub fn public_key_bytes(&self) -> &[u8; KEY_LEN] {
            &self.public_key
        }

        /// Computes the 32-byte X25519 Diffie-Hellman shared secret with `peer_public_key`.
        pub fn diffie_hellman(&self, peer_public_key: &[u8; KEY_LEN]) -> [u8; KEY_LEN] {
            derive_shared_secret(&self.private_key, peer_public_key)
        }
    }

    /// Computes the 32-byte X25519 Diffie-Hellman shared secret between `our_private_key` and `peer_public_key`.
    pub fn derive_shared_secret(
        our_private_key: &[u8; KEY_LEN],
        peer_public_key: &[u8; KEY_LEN],
    ) -> [u8; KEY_LEN] {
        let secret = StaticSecret::from(*our_private_key);
        let peer_pub = PublicKey::from(*peer_public_key);
        secret.diffie_hellman(&peer_pub).to_bytes()
    }
}

/// ChaCha20-Poly1305 AEAD payload encryption and authenticated decryption.
pub mod chacha20poly1305 {
    use super::*;
    use ::chacha20poly1305::{
        aead::{Aead, KeyInit},
        ChaCha20Poly1305, Key, Nonce,
    };

    /// Generates a cryptographically random 12-byte nonce via `getrandom`.
    pub fn generate_nonce() -> Result<[u8; NONCE_LEN], ProtocolError> {
        let mut nonce = [0u8; NONCE_LEN];
        getrandom::getrandom(&mut nonce)
            .map_err(|e| ProtocolError::RandomGenerationError(e.to_string()))?;
        Ok(nonce)
    }

    /// Encrypts and authenticates `plaintext` using `ChaCha20Poly1305` with an explicit 32-byte `key` and 12-byte `nonce`.
    pub fn seal_payload_with_nonce(
        key: &[u8; KEY_LEN],
        nonce: &[u8; NONCE_LEN],
        plaintext: &[u8],
    ) -> Result<SealedFrame, ProtocolError> {
        let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(nonce), plaintext)
            .map_err(|e| ProtocolError::EncryptionError(e.to_string()))?;

        Ok(SealedFrame {
            nonce: *nonce,
            ciphertext,
        })
    }

    /// Generates a fresh 12-byte random nonce and seals `plaintext` using `ChaCha20Poly1305`.
    pub fn seal_payload(
        key: &[u8; KEY_LEN],
        plaintext: &[u8],
    ) -> Result<SealedFrame, ProtocolError> {
        let nonce = generate_nonce()?;
        seal_payload_with_nonce(key, &nonce, plaintext)
    }

    /// Decrypts and verifies a `SealedFrame` using `ChaCha20Poly1305` with the given 32-byte `key`.
    /// Returns `Err(ProtocolError::DecryptionError(_))` if the authentication tag or ciphertext is invalid.
    pub fn open_payload(
        key: &[u8; KEY_LEN],
        frame: &SealedFrame,
    ) -> Result<Vec<u8>, ProtocolError> {
        let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
        cipher
            .decrypt(Nonce::from_slice(&frame.nonce), frame.ciphertext.as_ref())
            .map_err(|e| ProtocolError::DecryptionError(e.to_string()))
    }
}

pub use chacha20poly1305::{generate_nonce, open_payload, seal_payload, seal_payload_with_nonce};
pub use hmac_sha256::{
    compute_hmac_sha256, compute_invite_token, verify_hmac_sha256, verify_invite_token,
};
pub use x25519::{derive_shared_secret, X25519Keypair};
