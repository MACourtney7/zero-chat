//! Target-independent cryptographic primitives for zero_chat:
//! - HMAC-SHA256 invite token generation and constant-time verification
//! - X25519 ephemeral keypair generation and Diffie-Hellman key agreement with zeroization on drop
//! - Ed25519 signing keypair generation and signature verification with zeroization on drop
//! - Combined BurnerKeybundle for ephemeral zero-knowledge identities
//! - ChaCha20-Poly1305 AEAD payload sealing and opening (including blind dead-drop SDP helpers)
//! - Fast CSPRNG UUID generation without Curve25519 point multiplication

use crate::envelope::{InviteToken, ProtocolError, SealedFrame};

/// Standard 256-bit (32-byte) key length for X25519, Ed25519, and ChaCha20-Poly1305.
pub const KEY_LEN: usize = 32;

/// Standard 512-bit (64-byte) signature length for Ed25519.
pub const SIGNATURE_LEN: usize = 64;

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

/// Ed25519 signing keypair generation and signature verification.
pub mod ed25519 {
    use super::*;
    use ::ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
    use zeroize::{Zeroize, ZeroizeOnDrop};

    /// Ed25519 signing keypair whose private seed is automatically zeroized on drop.
    #[derive(Clone, Debug, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
    pub struct Ed25519Keypair {
        pub seed: [u8; KEY_LEN],
        #[zeroize(skip)]
        pub public_key: [u8; KEY_LEN],
    }

    impl Ed25519Keypair {
        /// Generates a fresh random Ed25519 keypair using `getrandom`.
        pub fn generate() -> Result<Self, ProtocolError> {
            let mut seed = [0u8; KEY_LEN];
            getrandom::getrandom(&mut seed)
                .map_err(|e| ProtocolError::RandomGenerationError(e.to_string()))?;
            let keypair = Self::from_seed(seed);
            seed.zeroize();
            Ok(keypair)
        }

        /// Deterministically constructs an `Ed25519Keypair` from a 32-byte secret seed.
        pub fn from_seed(seed: [u8; KEY_LEN]) -> Self {
            let signing_key = SigningKey::from_bytes(&seed);
            let public_key = signing_key.verifying_key().to_bytes();
            Self { seed, public_key }
        }

        /// Returns a reference to the 32-byte verifying public key.
        pub fn public_key_bytes(&self) -> &[u8; KEY_LEN] {
            &self.public_key
        }

        /// Signs `message` and returns a 64-byte Ed25519 signature.
        pub fn sign(&self, message: &[u8]) -> [u8; SIGNATURE_LEN] {
            let signing_key = SigningKey::from_bytes(&self.seed);
            signing_key.sign(message).to_bytes()
        }
    }

    /// Verifies `signature` over `message` against `public_key`.
    /// Returns `Ok(())` on success, or `Err(ProtocolError::InvalidHmacSignature)` on verification failure.
    pub fn verify_ed25519_signature(
        public_key: &[u8; KEY_LEN],
        message: &[u8],
        signature: &[u8; SIGNATURE_LEN],
    ) -> Result<(), ProtocolError> {
        let verifying_key = VerifyingKey::from_bytes(public_key)
            .map_err(|_| ProtocolError::InvalidHmacSignature)?;
        let sig = Signature::from_bytes(signature);
        verifying_key
            .verify(message, &sig)
            .map_err(|_| ProtocolError::InvalidHmacSignature)
    }
}

/// Ephemeral burner keybundle combining X25519 key agreement and Ed25519 signature keypairs.
pub mod burner {
    use super::*;
    use crate::crypto::{ed25519::Ed25519Keypair, x25519::X25519Keypair};
    use zeroize::{Zeroize, ZeroizeOnDrop};

    /// Ephemeral burner keybundle combining X25519 key agreement and Ed25519 signature keypairs.
    #[derive(Clone, Debug, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
    pub struct BurnerKeybundle {
        pub x25519: X25519Keypair,
        pub ed25519: Ed25519Keypair,
    }

    impl BurnerKeybundle {
        /// Generates fresh random X25519 and Ed25519 keypairs.
        pub fn generate() -> Result<Self, ProtocolError> {
            Ok(Self {
                x25519: X25519Keypair::generate()?,
                ed25519: Ed25519Keypair::generate()?,
            })
        }

        /// Returns the 32-byte X25519 public key.
        pub fn x25519_public_key(&self) -> [u8; KEY_LEN] {
            *self.x25519.public_key_bytes()
        }

        /// Returns the 32-byte Ed25519 public key.
        pub fn ed25519_public_key(&self) -> [u8; KEY_LEN] {
            *self.ed25519.public_key_bytes()
        }

        /// Explicitly zeroizes both secret key materials in memory.
        pub fn zeroize(&mut self) {
            self.x25519.zeroize();
            self.ed25519.zeroize();
        }
    }
}

/// Cryptographically secure random UUID v4 generation without Curve25519 point multiplication.
pub mod uuid {
    use super::*;

    /// Generates 16 cryptographically random bytes via `getrandom`.
    pub fn generate_random_bytes_16() -> Result<[u8; 16], ProtocolError> {
        let mut bytes = [0u8; 16];
        getrandom::getrandom(&mut bytes)
            .map_err(|e| ProtocolError::RandomGenerationError(e.to_string()))?;
        Ok(bytes)
    }

    /// Generates an RFC 4122 v4 UUID string (`xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx`)
    /// using direct CSPRNG bytes.
    pub fn generate_uuid_v4() -> Result<String, ProtocolError> {
        let mut bytes = generate_random_bytes_16()?;
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
            signature: None,
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

    /// Encrypts `plaintext` into a `SealedFrame` using the derived 32-byte `shared_secret`.
    pub fn seal_with_shared_secret(
        shared_secret: &[u8; KEY_LEN],
        plaintext: &[u8],
    ) -> Result<SealedFrame, ProtocolError> {
        seal_payload(shared_secret, plaintext)
    }

    /// Decrypts a `SealedFrame` using the derived 32-byte `shared_secret`.
    pub fn open_with_shared_secret(
        shared_secret: &[u8; KEY_LEN],
        frame: &SealedFrame,
    ) -> Result<Vec<u8>, ProtocolError> {
        open_payload(shared_secret, frame)
    }
}

pub use burner::BurnerKeybundle;
pub use chacha20poly1305::{
    generate_nonce, open_payload, open_with_shared_secret, seal_payload, seal_payload_with_nonce,
    seal_with_shared_secret,
};
pub use ed25519::{verify_ed25519_signature, Ed25519Keypair};
pub use hmac_sha256::{
    compute_hmac_sha256, compute_invite_token, verify_hmac_sha256, verify_invite_token,
};
pub use uuid::{generate_random_bytes_16, generate_uuid_v4};
pub use x25519::{derive_shared_secret, X25519Keypair};
