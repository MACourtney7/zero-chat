# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-046"
title: "Option B Burner Keybundle (Ed25519 + X25519 + ChaCha20-Poly1305), Blind Dead-Drop Signaling & WASM Crypto Bridge"
state: "PLANNING"
created_at: "2026-10-26T11:15:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-046/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

As specified in [`docs/HANDOVER_TICKET_046_OPTION_B_RUST_CRYPTO_AND_WASM_BRIDGE.md`](file:///Users/mattcourtney/Library/CloudStorage/GoogleDrive-mattadam.courtney@gmail.com/My%20Drive/zero_chat/docs/HANDOVER_TICKET_046_OPTION_B_RUST_CRYPTO_AND_WASM_BRIDGE.md):

Currently, `zero_protocol` implements raw `x25519-dalek` Diffie-Hellman key agreement and `chacha20poly1305` symmetric AEAD sealing, but `DropPayload` transmits the Joiner's WebRTC `sdp_answer` in cleartext JSON through the Cloudflare Worker (`zero_network`). Furthermore, multi-peer mesh introductions (`PeerJoined`) lack cryptographic signatures to authenticate peer identities across relay hops, and `zero_client/index.html` relies on stubbed byte payloads with dummy `Array(32).fill(0)` keys.

`ticket-046` implements the **Option B Modernized Burner Keybundle** entirely in Rust (`zero_protocol` and `zero_client`) and exposes it via `#[wasm_bindgen]` so that subsequent tickets (e.g. `ticket-047`) can cleanly swap out JavaScript stubs in `zero_client/index.html` without cryptographic ambiguity.

### Core Cryptographic Enhancements
1. **Ephemeral Burner Identity (`Ed25519Keypair` + `X25519Keypair`)**: Each client session generates an in-memory `BurnerKeybundle` combining an `x25519-dalek` Diffie-Hellman keypair (for pairwise ChaCha20-Poly1305 AEAD encryption) and an `ed25519-dalek` signing keypair (for authenticating mesh introductions and envelopes), backed by `Zeroize` and `ZeroizeOnDrop`.
2. **Blind Dead-Drop Signaling (`sealed_sdp_answer`)**: Because the Joiner receives `host_public_key: [u8; 32]` from the `#payload=` URL fragment and sends `joiner_public_key: [u8; 32]` in `DropPayload`, both parties can immediately derive the pairwise X25519 shared secret. The Joiner seals `sdp_answer.sdp` into `sealed_sdp_answer: Option<SealedFrame>` prior to POSTing to `/drop/:do_id/:invite_uuid`. The Cloudflare Worker (`zero_network`) becomes 100% blind to the SDP answer and internal ICE candidates.
3. **Signed Mesh Introductions & Authenticated Frames (`Ed25519`)**: `PeerJoined` introductions and `SealedFrame` payloads carry verifiable Ed25519 signatures, preventing intermediate relay peers in a multi-user mesh from spoofing peer identities or tampering with introduced public keys.
4. **Full `#[wasm_bindgen]` Crypto Bridge (`WasmCryptoSession`)**: Exposes a complete, ergonomic WASM API in `zero_client` ready for browser consumption.
5. **CSPRNG 16-byte UUID Generation**: Replaces Curve25519 scalar multiplication inside UUID generation with a fast, direct 16-byte CSPRNG helper `generate_random_bytes_16()`.

---

## 2. Requirements & Acceptance Criteria

### REQ-046.1: `ed25519-dalek` Dependency & `BurnerKeybundle` in `zero_protocol::crypto`
- In `crates/zero_protocol/Cargo.toml`:
  - Add `ed25519-dalek = { version = "2", default-features = false, features = ["rand_core", "zeroize"] }`.
- In `crates/zero_protocol/src/crypto.rs`:
  - Implement `Ed25519Keypair`: wraps a 32-byte secret signing seed and 32-byte verifying public key (`[u8; 32]`), implementing `Zeroize` and `ZeroizeOnDrop` for secret material.
  - Implement `sign(&self, message: &[u8]) -> [u8; 64]` and `verify_ed25519_signature(public_key: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> Result<(), ProtocolError>`.
  - Implement `BurnerKeybundle`:
    ```rust
    pub struct BurnerKeybundle {
        pub x25519: X25519Keypair,
        pub ed25519: Ed25519Keypair,
    }
    ```
    - `BurnerKeybundle::generate() -> Result<Self, ProtocolError>`
    - `x25519_public_key(&self) -> [u8; 32]`
    - `ed25519_public_key(&self) -> [u8; 32]`
    - `zeroize(&mut self)` explicitly zeroizing both secret keys in memory.
  - Implement `generate_random_bytes_16() -> Result<[u8; 16], ProtocolError>` using `getrandom` and update RFC 4122 v4 UUID generation to eliminate Curve25519 point multiplication overhead.

### REQ-046.2: Blind Dead-Drop SDP Encryption
- In `crates/zero_protocol/src/crypto.rs`:
  - Implement pairwise shared secret sealing helpers:
    ```rust
    pub fn seal_with_shared_secret(shared_secret: &[u8; 32], plaintext: &[u8]) -> Result<SealedFrame, ProtocolError>
    pub fn open_with_shared_secret(shared_secret: &[u8; 32], frame: &SealedFrame) -> Result<Vec<u8>, ProtocolError>
    ```
  - When Joiner prepares `DropPayload`: encrypts `sdp_answer.sdp` into `sealed_sdp_answer: Option<SealedFrame>` using the derived X25519 shared secret between `joiner_private_key` and `host_public_key`, setting `sdp_answer.sdp` to `"SEALED"`.
  - When Host receives `DropPayload`: derives the shared secret using `drop.joiner_public_key` and decrypts `sealed_sdp_answer` to recover the plaintext SDP string.
  - Fail-closed: returns `ProtocolError::DecryptionError` if ciphertext or nonce is modified or tampered with.

### REQ-046.3: Backward-Compatible Wire Envelope Extensions
- In `crates/zero_protocol/src/envelope.rs`:
  - `InvitePayload`:
    ```rust
    #[serde(default)]
    pub host_signing_key: Option<[u8; 32]>,
    ```
  - `DropPayload`:
    ```rust
    #[serde(default)]
    pub joiner_signing_key: Option<[u8; 32]>,
    #[serde(default)]
    pub sealed_sdp_answer: Option<SealedFrame>,
    #[serde(default)]
    pub signature: Option<Vec<u8>>, // 64-byte Ed25519 signature over (invite_uuid || joiner_public_key || ciphertext)
    ```
  - `SealedFrame`:
    ```rust
    pub struct SealedFrame {
        pub nonce: [u8; 12],
        pub ciphertext: Vec<u8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub signature: Option<Vec<u8>>, // 64-byte Ed25519 signature over (nonce || ciphertext)
    }
    ```
  - `MeshRelaySignal::PeerJoined`:
    ```rust
    PeerJoined {
        peer_id: String,
        public_key: [u8; 32],
        #[serde(default)]
        signing_key: Option<[u8; 32]>,
        #[serde(default)]
        signature: Option<Vec<u8>>,
    }
    ```
  - Ensure all deserialization tests from legacy payloads continue to succeed without schema breakage.

### REQ-046.4: `#[wasm_bindgen]` Crypto Bridge (`WasmCryptoSession`)
- In `zero_client/src/crypto.rs` (and re-exported from `zero_client/src/lib.rs`):
  - Implement `#[wasm_bindgen] pub struct WasmCryptoSession`:
    ```rust
    #[wasm_bindgen]
    pub struct WasmCryptoSession {
        bundle: BurnerKeybundle,
        peer_sessions: HashMap<String, PeerCryptoEntry>,
    }
    ```
  - Methods:
    - `#[wasm_bindgen(constructor)] pub fn new() -> Result<WasmCryptoSession, JsValue>`
    - `pub fn x25519_public_key_bytes(&self) -> Vec<u8>` (32 bytes)
    - `pub fn ed25519_public_key_bytes(&self) -> Vec<u8>` (32 bytes)
    - `pub fn register_peer(&mut self, peer_id: &str, peer_x25519_pub: &[u8], peer_ed25519_pub: Option<Vec<u8>>) -> Result<(), JsValue>`
    - `pub fn has_peer_session(&self, peer_id: &str) -> bool`
    - `pub fn remove_peer(&mut self, peer_id: &str)` (drops and zeroizes session key)
    - `pub fn seal_for_peer(&self, peer_id: &str, plaintext: &[u8]) -> Result<String, JsValue>` (encrypts ChaCha20-Poly1305, signs with Ed25519, returns JSON `SealedFrame`)
    - `pub fn open_from_peer(&self, peer_id: &str, sealed_frame_json: &str) -> Result<Vec<u8>, JsValue>` (verifies signature, decrypts, returns plaintext bytes)
    - `pub fn seal_drop_sdp(&mut self, host_peer_id: &str, host_x25519_pub: &[u8], host_ed25519_pub: Option<Vec<u8>>, sdp_plaintext: &str) -> Result<String, JsValue>`
    - `pub fn open_drop_sdp(&mut self, joiner_peer_id: &str, joiner_x25519_pub: &[u8], joiner_ed25519_pub: Option<Vec<u8>>, sealed_frame_json: &str) -> Result<String, JsValue>`
    - `pub fn sign_bytes(&self, data: &[u8]) -> Vec<u8>`
    - `pub fn verify_peer_signature(&self, peer_id: &str, data: &[u8], sig: &[u8]) -> Result<bool, JsValue>`
    - `pub fn compute_hmac_hex(secret: &[u8], invite_uuid: &str) -> Result<String, JsValue>`
    - `pub fn burn_all(&mut self)` (clears peer sessions and zeroizes bundle)

### REQ-046.5: Automated Verification & Dry-Run Integration
- In `crates/zero_protocol/tests/protocol_tests.rs`:
  - `test_burner_keybundle_generation_and_zeroize`: key generation, signing/verification, zeroization.
  - `test_blind_dead_drop_sdp_encryption_and_tamper_rejection`: blind SDP encryption, host decryption, and fail-closed on tamper.
  - `test_signed_sealed_frame_authenticity`: authentic Ed25519 signature passes; forged signature fails.
  - `test_backward_compatible_envelopes`: legacy payloads deserialize with `None` values and pass cleanly.
- Workspace Quality Gates:
  - `cargo test --workspace` (100% passing across all crates).
  - `cargo check --target wasm32-unknown-unknown -p zero_client` & `zero_network`.
  - `cargo fmt -- --check` & `cargo clippy --workspace --all-targets -- -D warnings`.
  - `./scripts/deploy_dry_run.sh`: builds release WASM, executes `wasm-bindgen`, and confirms `WasmCryptoSession` is exported.
  - `bash zero_client/tests/browser/run_test.sh`: all existing 21 browser tests pass without regression.

---

## 3. Target File Scope & Blast Radius

| File | Subsystem | Planned Change |
| :--- | :--- | :--- |
| `crates/zero_protocol/Cargo.toml` | Protocol Engine | Add `ed25519-dalek` with `rand_core` & `zeroize` features |
| `crates/zero_protocol/src/crypto.rs` | Protocol Engine | Implement `Ed25519Keypair`, `BurnerKeybundle`, blind SDP helpers, and CSPRNG UUID bytes |
| `crates/zero_protocol/src/envelope.rs` | Protocol Engine | Add backward-compatible fields (`signing_key`, `signature`, `sealed_sdp_answer`) |
| `crates/zero_protocol/tests/protocol_tests.rs` | Protocol Tests | Unit tests for burner bundle, blind SDP encryption, signed frames, and backward compatibility |
| `zero_client/src/crypto.rs` | Client Crypto | Implement `WasmCryptoSession` and peer session management with Ed25519 verification |
| `zero_client/src/lib.rs` | Client Root | Export `WasmCryptoSession` via `#[wasm_bindgen]` |

---

## 4. Subagent Brief Decomposition & Sizing

| Brief | Cognitive Sizing | Scope & Role | Target Files |
| :--- | :---: | :--- | :--- |
| `brief-impl-046-protocol.md` | `MEDIUM` [M] | Implement `ed25519-dalek`, `Ed25519Keypair`, `BurnerKeybundle`, blind SDP sealing, envelope extensions, and protocol tests | `crates/zero_protocol/**` |
| `brief-impl-046-wasm.md` | `MEDIUM` [M] | Implement `WasmCryptoSession` in `zero_client`, exposing full crypto bridge to JavaScript | `zero_client/src/crypto.rs`, `zero_client/src/lib.rs` |
| `brief-validation.md` | `MEDIUM` [M] | Comprehensive quality gates: `cargo test`, dual wasm compilation, clippy, dry-run, browser regression | Workspace-wide |
| `brief-review-1.md` | `HIGH` [H] | Independent high-scrutiny code review: zeroization audit, timing invariants, backward compatibility | All modified files |

---

## 5. Security & Invariant Safeguards

1. **Zero Forensic Footprint & Volatile Zeroization**:
   - `Ed25519Keypair`, `X25519Keypair`, and `BurnerKeybundle` implement `Zeroize` and `ZeroizeOnDrop`.
   - `burn_all()` wipes both secret keys and all active symmetric session keys in WASM linear memory.
2. **Blind Cloud Edge**:
   - Cloudflare Worker (`zero_network`) handles only dead-drop routing; with `sealed_sdp_answer`, the worker cannot inspect SDP parameters, IP candidates, or codec profiles.
3. **Fail-Closed Authenticated Envelopes**:
   - Tampered signatures or corrupted Poly1305 tags immediately abort with `ProtocolError::DecryptionError` or `ProtocolError::InvalidHmacSignature`, preventing forged relay injections.
