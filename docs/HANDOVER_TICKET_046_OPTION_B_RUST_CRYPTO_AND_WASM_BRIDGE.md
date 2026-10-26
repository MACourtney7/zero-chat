Architectural Handover Brief: Option B Burner Keybundle (Ed25519 + X25519 + ChaCha20-Poly1305), Blind Dead-Drop & WASM Crypto Bridge (ticket-046)
1. Executive Summary & Objective
Currently, zero_protocol implements raw x25519-dalek Diffie-Hellman key agreement and chacha20poly1305 symmetric sealing, while zero_client/index.html stubs out SealedFrame payloads with unencrypted UTF-8 bytes and dummy Array(32).fill(0) public keys. Furthermore, DropPayload sends the joiner's WebRTC sdp_answer in cleartext JSON through the Cloudflare Worker (zero_network), and multi-peer mesh introductions (PeerJoined) lack cryptographic signatures to authenticate peer identities across relay hops.

ticket-046 implements the Option B Modernized Burner Keybundle entirely in Rust (zero_protocol and zero_client) and exposes it via #[wasm_bindgen] so that ticket-047 can cleanly swap out all JavaScript stubs in zero_client/index.html.
Core Cryptographic Enhancements in ticket-046
Ephemeral Burner Identity (Ed25519 Signing + X25519 Encryption): Each client session generates an in-memory BurnerKeybundle combining an x25519-dalek Diffie-Hellman keypair (for pairwise ChaCha20-Poly1305 AEAD encryption) and an ed25519-dalek signing keypair (for authenticating mesh introductions and envelopes), backed by ZeroizeOnDrop.
Blind Dead-Drop Signaling (sealed_sdp_answer): Using the Host's host_public_key and the pre-distributed burner guest_private_key (or the Joiner's ephemeral X25519 keypair + host_public_key), the Joiner encrypts sdp_answer inside a SealedFrame before POSTing DropPayload to /drop/:do_id/:invite_uuid. The Cloudflare Worker (zero_network) becomes 100% blind to the SDP answer and ICE IP candidates.
Signed Mesh Introductions & Authenticated Frames (Ed25519): PeerJoined introductions and SealedFrame payloads carry verifiable Ed25519 signatures, preventing intermediate relay peers in a multi-user mesh from spoofing peer identities or tampering with introduced public keys.
Full #[wasm_bindgen] Crypto Bridge (WasmCryptoSession): Exposes a complete, ergonomic WASM API in zero_client ready for zero_client/index.html to consume in ticket-047.


2. Target File Scope
Cargo.toml & crates/zero_protocol/Cargo.toml (Add ed25519-dalek = { version = "2", default-features = false, features = ["rand_core", "zeroize"] })
crates/zero_protocol/src/crypto.rs (Add Ed25519Keypair, BurnerKeybundle, sign_message, verify_signature, seal_blind_drop_sdp, open_blind_drop_sdp, and fast CSPRNG 16-byte UUID generation without Curve25519 point multiplication)
crates/zero_protocol/src/envelope.rs (Add backward-compatible #[serde(default)] fields for ed25519_public_key, signature, and sealed_sdp_answer on InvitePayload, DropPayload, PeerJoined, and SealedFrame)
crates/zero_protocol/tests/protocol_tests.rs (Unit tests for BurnerKeybundle, Ed25519 signatures, blind dead-drop SDP encryption, and backward-compatible serde serialization)
zero_client/src/crypto.rs & zero_client/src/lib.rs (Implement and export #[wasm_bindgen] pub struct WasmCryptoSession)


3. Detailed Technical Specification
Part A: Ephemeral BurnerKeybundle (crates/zero_protocol/src/crypto.rs)
Add ed25519-dalek Dependency: In crates/zero_protocol/Cargo.toml, add ed25519-dalek with zeroize support.
Implement Ed25519Keypair and BurnerKeybundle:
Ed25519Keypair wraps a 32-byte signing seed and 32-byte verifying public key ([u8; 32]), deriving Zeroize and ZeroizeOnDrop on the secret seed.
Provide sign(&self, message: &[u8]) -> [u8; 64] and verify_ed25519_signature(public_key: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> Result<(), ProtocolError>.
BurnerKeybundle combines x25519: X25519Keypair and ed25519: Ed25519Keypair, providing:
BurnerKeybundle::generate() -> Result<Self, ProtocolError>
x25519_public_key(&self) -> [u8; 32]
ed25519_public_key(&self) -> [u8; 32]
zeroize(&mut self) (explicitly zeroizing both secret keys in WASM linear memory).
Blind Dead-Drop SDP Encryption (seal_drop_sdp / open_drop_sdp):
Because the Joiner knows host_public_key: [u8; 32] (from #payload=) and sends joiner_public_key: [u8; 32] in DropPayload, the Joiner and Host can immediately derive the pairwise X25519 shared secret derive_shared_secret(&joiner_private_key, &host_public_key) during the dead-drop handshake itself!
Implement:
seal_with_shared_secret(shared_secret: &SharedSecretKey, plaintext: &[u8]) -> Result<SealedFrame, ProtocolError>
open_with_shared_secret(shared_secret: &SharedSecretKey, frame: &SealedFrame) -> Result<Vec<u8>, ProtocolError>
When the Joiner constructs DropPayload, it seals the sdp_answer.sdp string into sealed_sdp_answer: Option<SealedFrame> using the derived SharedSecretKey, and sets sdp_answer.sdp to "" (or "SEALED").
When the Host receives DropPayload over the WebSocket, if sealed_sdp_answer is Some(ref frame), the Host derives the SharedSecretKey using drop.joiner_public_key and decrypts frame to recover the real SDP answer string. The Cloudflare Worker (zero_network) never sees the plaintext SDP answer!
Optimize UUID Generation (generate_random_uuid_v4):
Replace the Curve25519 scalar multiplication inside zero_client::crypto::generate_message_uuid and zero_client::invite::generate_uuid_v4 with a direct 16-byte CSPRNG helper generate_random_bytes_16() -> Result<[u8; 16], ProtocolError> in zero_protocol::crypto.


Part B: Wire Envelope Extensions (crates/zero_protocol/src/envelope.rs)
Preserve 100% backward compatibility with existing tests by using #[serde(default)] on all new fields:

InvitePayload:

#[serde(default)]

pub host_signing_key: Option<[u8; 32]>,

DropPayload:

#[serde(default)]

pub joiner_signing_key: Option<[u8; 32]>,

#[serde(default)]

pub sealed_sdp_answer: Option<SealedFrame>,

#[serde(default)]

pub signature: Option<Vec<u8>>, // 64-byte Ed25519 signature over (invite_uuid || joiner_public_key || sealed_sdp_answer.ciphertext)

SealedFrame:

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]

pub struct SealedFrame {

    pub nonce: [u8; 12],

    pub ciphertext: Vec<u8>,

    #[serde(default, skip_serializing_if = "Option::is_none")]

    pub signature: Option<Vec<u8>>, // Optional 64-byte Ed25519 sender signature over (nonce || ciphertext)

}

PeerJoined (inside MeshSignal / zero_client::mesh):
Ensure PeerJoined carries public_key: [u8; 32] (X25519), #[serde(default)] signing_key: Option<[u8; 32]> (Ed25519), and #[serde(default)] signature: Option<Vec<u8>> so introduced peers can verify the introduction and derive pairwise X25519 keys for RelaySdpOffer / RelaySdpAnswer and BroadcastChat.


Part C: #[wasm_bindgen] Crypto Bridge (zero_client/src/crypto.rs & lib.rs)
Export a #[wasm_bindgen] pub struct WasmCryptoSession in zero_client so the browser runtime can invoke all cryptographic operations directly in WASM memory:

#[wasm_bindgen]

pub struct WasmCryptoSession {

    bundle: BurnerKeybundle,

    peer_sessions: HashMap<String, PeerCryptoEntry>,

}

Required #[wasm_bindgen] methods on WasmCryptoSession:

#[wasm_bindgen(constructor)] pub fn new() -> Result<WasmCryptoSession, JsValue>
Generates a fresh BurnerKeybundle (X25519 + Ed25519) in WASM RAM.
pub fn x25519_public_key_bytes(&self) -> Vec<u8> (32 bytes)
pub fn ed25519_public_key_bytes(&self) -> Vec<u8> (32 bytes)
pub fn register_peer(&mut self, peer_id: &str, peer_x25519_pub: &[u8], peer_ed25519_pub: Option<Vec<u8>>) -> Result<(), JsValue>
Performs X25519 Diffie-Hellman with self.bundle.x25519 and peer_x25519_pub, storing the resulting ChatSessionCrypto (ChaCha20-Poly1305 cipher) and optional peer_ed25519_pub verifying key under peer_id.
pub fn has_peer_session(&self, peer_id: &str) -> bool
pub fn remove_peer(&mut self, peer_id: &str)
Drops and zeroizes the session key for peer_id.
pub fn seal_for_peer(&self, peer_id: &str, plaintext: &[u8]) -> Result< String, JsValue >
Encrypts plaintext with the peer's ChatSessionCrypto (ChaCha20-Poly1305), signs (nonce || ciphertext) with self.bundle.ed25519, and returns the serialized JSON SealedFrame { nonce, ciphertext, signature }.
pub fn open_from_peer(&self, peer_id: &str, sealed_frame_json: &str) -> Result<Vec<u8>, JsValue>
Deserializes SealedFrame, verifies the Ed25519 signature if peer_ed25519_pub is registered for peer_id, and decrypts + verifies the Poly1305 MAC tag using ChaCha20-Poly1305, returning the plaintext bytes.
pub fn seal_drop_sdp(&mut self, host_peer_id: &str, host_x25519_pub: &[u8], host_ed25519_pub: Option<Vec<u8>>, sdp_plaintext: &str) -> Result<String, JsValue>
Registers the host peer session, seals sdp_plaintext into a SealedFrame JSON string for DropPayload.sealed_sdp_answer.
pub fn open_drop_sdp(&mut self, joiner_peer_id: &str, joiner_x25519_pub: &[u8], joiner_ed25519_pub: Option<Vec<u8>>, sealed_frame_json: &str) -> Result<String, JsValue>
Registers the joiner peer session on the host, decrypts sealed_sdp_answer, and returns the plaintext SDP string.
pub fn sign_bytes(&self, data: &[u8]) -> Vec<u8> & pub fn verify_peer_signature(&self, peer_id: &str, data: &[u8], sig: &[u8]) -> Result<bool, JsValue>
pub fn compute_hmac_hex(secret: &[u8], invite_uuid: &str) -> Result<String, JsValue>
pub fn burn_all(&mut self)
Clears all peer_sessions and calls self.bundle.zeroize().


4. Verification & Acceptance Criteria
Gate 1: Cryptographic & Protocol Unit Tests (zero_protocol)
test_burner_keybundle_generation_and_zeroize: Verifies X25519 + Ed25519 key generation, signing/verification, and zeroization of secret material.
test_blind_dead_drop_sdp_encryption_and_tamper_rejection: Verifies that DropPayload with sealed_sdp_answer hides the plaintext SDP from intermediate inspection, decrypts cleanly on the host, and fails closed (ProtocolError::DecryptionFailed) if any ciphertext or nonce byte is modified.
test_signed_sealed_frame_authenticity: Verifies that SealedFrame signed by Peer A's Ed25519 key verifies cleanly and rejects forged signatures from Peer C.
test_backward_compatible_envelopes: Existing InvitePayload and DropPayload payloads without sealed_sdp_answer or signature continue to pass all existing protocol and network tests.
Gate 2: Client & Workspace Integration Suite
cargo test --workspace — All workspace tests across zero_protocol, zero_client, and zero_network must pass with 0 failures.
./scripts/deploy_dry_run.sh — Must compile zero_client to wasm32-unknown-unknown, run wasm-bindgen --target web --out-dir dist/pkg, and verify that WasmCryptoSession is exported in zero_client/dist/pkg/zero_client.js.
