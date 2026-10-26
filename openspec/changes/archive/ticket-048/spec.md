# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-048"
title: "Consolidate & Prune Legacy Rust Client Scaffolding"
state: "COMPLETED"
created_at: "2026-10-26T21:45:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-048/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

With `ticket-046` and `ticket-047` complete, ZeroChat's production architecture has reached a clean, natural separation of concerns:
1. **Rust/WASM High-Assurance Core (`zero_protocol` & `zero_client`)**: Executes 100% of the cryptography (`BurnerKeybundle` with `x25519-dalek` + `ed25519-dalek`, `ChaCha20-Poly1305` AEAD per-peer session sealing, blind dead-drop SDP encryption, HMAC token generation, and `ZeroizeOnDrop` memory wiping via `WasmCryptoSession`) alongside canonical wire protocol serialization.
2. **Browser DOM, Viewport & WebRTC Orchestrator (`zero_client/index.html` & `zero_client/style.css`)**: Handles the glassmorphic UI, mobile `window.visualViewport` keyboard geometry, synchronous touch clipboard gestures, pre-assigned peer IDs (`assigned_peer_id`), per-peer role tracking (`peerRoles`), and the 12-second mobile background freeze grace window (`pendingDisconnectTimers`).

However, `zero_client/src/` still carries ~2,500 lines of obsolete pre-ticket-032 scaffolding (`ui.rs`, `bootstrap.rs`, and legacy unintegrated DOM/WebRTC glue in `webrtc.rs`). Specifically:
- `zero_client/src/ui.rs` contains a hardcoded, outdated HTML string template (`ChatUi::mount`) and dead DOM bindings that were superseded by `zero_client/index.html`.
- `zero_client/src/bootstrap.rs` contains legacy DOM initialization routines that are never invoked by the production application.
- `zero_client/src/webrtc.rs` contains unconfigured `web-sys` RTC stubs lacking STUN/TURN servers and smart ICE candidate gathering.
- `zero_client/Cargo.toml` includes 45+ unused `web-sys` feature flags for DOM elements.

`ticket-048` consolidates `zero_client` into a lean, purpose-built WASM Cryptography, Protocol & Pure-State library—pruning dead DOM/WebRTC glue, upgrading the pure-Rust state machine (`mesh.rs`) to reflect the battle-tested `ticket-045` resilience invariants (`peerRoles` and disconnect grace periods), slimming `Cargo.toml`, and shrinking the compiled `.wasm` binary footprint.

---

## 2. Requirements & Acceptance Criteria

### REQ-048.1: Prune Obsolete DOM & Bootstrap Modules (`ui.rs`, `bootstrap.rs`)
- Remove `zero_client/src/ui.rs` and `zero_client/src/bootstrap.rs`.
- Remove `pub mod ui;` and `pub mod bootstrap;` (and their glob re-exports) from `zero_client/src/lib.rs`.
- Expose a clean, well-documented public API in `zero_client/src/lib.rs` centered on:
  - `crypto::WasmCryptoSession` and cryptographic primitives (`ChatSessionCrypto`, etc.)
  - `invite` (offline invite token generation, fragment packing/unpacking)
  - `mesh` (`MeshTopology`, `PeerRole`, etc.)
  - `webrtc` (pure state types: `PeerLifecycleState`, `SdpType`, etc.)

### REQ-048.2: Replace Heavy `web-sys` RTC Glue & Slim Down `Cargo.toml`
- In `zero_client/src/webrtc.rs`:
  - Retain pure-Rust state machines and validation primitives (`PeerLifecycleState`, `SdpType`, `WebRtcError`, SDP validation helpers).
  - Remove unused `#[cfg(target_arch = "wasm32")]` `web-sys` `RtcPeerConnection`, `RtcDataChannel`, and associated browser event closures that are redundant with `ZeroWebRtcEngine` in `index.html`.
- In `zero_client/Cargo.toml`:
  - Strip obsolete `web-sys` feature flags (`Document`, `Element`, `HtmlElement`, `HtmlInputElement`, `HtmlButtonElement`, `HtmlTextAreaElement`, `KeyboardEvent`, `MouseEvent`, `Window`, `Storage`, `Node`, `RtcPeerConnection`, `RtcDataChannel`, etc.).
  - Retain only the minimal dependencies needed for WASM bindings and protocol handling.

### REQ-048.3: Align `zero_client/src/mesh.rs` with Production Invariants
- In `zero_client/src/mesh.rs`, align `MeshTopology` with the production invariants proven in `ticket-045` and `ticket-047`:
  - **Per-Peer Role Tracking**:
    ```rust
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum PeerRole {
        Inviter,
        Joiner,
        Mesh,
    }
    ```
    - Track `peer_roles: HashMap<String, PeerRole>` inside `MeshTopology`.
    - Provide `set_peer_role(&mut self, peer_id: &str, role: PeerRole)` and `peer_role(&self, peer_id: &str) -> Option<PeerRole>`.
  - **Transient Disconnect Grace State**:
    - Provide `mark_peer_disconnected_grace(&mut self, peer_id: &str, timestamp_ms: u64)`: records the peer as experiencing transient disconnect without prematurely purging messages or emitting `on_peer_left`.
    - Provide `recover_peer(&mut self, peer_id: &str)`: cancels pending grace state and removes `peer_id` from departed sets upon channel re-establishment.
    - Provide `expire_peer_grace(&mut self, peer_id: &str)`: triggers definitive departure, firing selective message burn and `on_peer_left` callbacks.
    - Explicit `PeerLeftAndBurn` continues to immediately bypass grace and burn without delay.

### REQ-048.4: Test Suite Refactoring & Native Test Doubles
- Update existing native integration tests (`zero_client/tests/mesh_simulation_test.rs`, `zero_client/tests/e2e_handshake.rs`, `zero_client/tests/webrtc_tests.rs`, `zero_client/tests/invite_tests.rs`):
  - Replace any dependencies on `zero_client::ChatUi` or `zero_client::bootstrap` with local test doubles or direct `MeshTopology` / `ChatSessionCrypto` assertions.
  - Retain full functional coverage of multi-node mesh simulations, broadcast flooding, loop suppression, selective message burning, and symmetric counter updates.
  - Ensure 100% test pass rate across `cargo test --workspace`.

### REQ-048.5: Verification Gates & Spec Reconciliation
- **Gate 1: Rust Workspace Tests & Clippy**:
  - `cargo test --workspace` must pass with 0 failures across `zero_protocol`, `zero_client`, and `zero_network`.
  - `cargo clippy --workspace --all-targets -- -D warnings` must pass with 0 warnings.
  - `cargo fmt -- --check` must pass cleanly.
- **Gate 2: WASM Build & Bundle Verification**:
  - `./scripts/deploy_dry_run.sh` must succeed, generating `dist/pkg/zero_client_bg.wasm` and staging assets cleanly.
  - Verify that `WasmCryptoSession` exports remain 100% intact and functional.
- **Gate 3: Browser Headless Regression Suite**:
  - `bash zero_client/tests/browser/run_test.sh` must pass all 22 tests (including Test 22 Scenarios A–E) with 0 regressions.
- **Canonical Architecture Specs**:
  - Update `openspec/specs/overview.md` to accurately document the production division of responsibilities and pruned Rust architecture.

---

## 3. Cognitive Sizing & Brief Decomposition

| Brief ID | Subsystem Target | Files Impacted | Cognitive Level |
| :--- | :--- | :--- | :---: |
| `brief-impl-048-prune` | Dead DOM/WebRTC Pruning & Cargo Cleanup | `zero_client/src/lib.rs`, `zero_client/src/ui.rs`, `zero_client/src/bootstrap.rs`, `zero_client/src/webrtc.rs`, `zero_client/Cargo.toml` | `MEDIUM` [M] |
| `brief-impl-048-mesh` | Mesh State Machine & PeerRole/Grace Alignment | `zero_client/src/mesh.rs` | `MEDIUM` [M] |
| `brief-impl-048-tests` | Native Test Suite Migration & Test Double Refactor | `zero_client/tests/mesh_simulation_test.rs`, `zero_client/tests/e2e_handshake.rs`, `zero_client/tests/webrtc_tests.rs` | `MEDIUM` [M] |

---

## 4. Verification Commands

```bash
cargo fmt -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/deploy_dry_run.sh
bash zero_client/tests/browser/run_test.sh
```
