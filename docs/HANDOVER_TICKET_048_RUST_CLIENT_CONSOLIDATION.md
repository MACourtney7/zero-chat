Architectural Handover Brief: Consolidate & Prune Legacy Rust Client Scaffolding (ticket-048)
1. Executive Summary & Objective
With ticket-046 and ticket-047 complete, ZeroChat's production architecture has reached a clean, natural separation of concerns:

Rust/WASM High-Assurance Core (zero_protocol & zero_client): Executes 100% of the cryptography (BurnerKeybundle with x25519-dalek + ed25519-dalek, ChaCha20-Poly1305 AEAD per-peer session sealing, blind dead-drop SDP encryption, HMAC token generation, and ZeroizeOnDrop memory wiping via WasmCryptoSession) alongside canonical wire protocol serialization.
Browser DOM, Viewport & WebRTC Orchestrator (zero_client/index.html & zero_client/style.css): Handles the glassmorphic UI, mobile window.visualViewport keyboard geometry, synchronous touch clipboard gestures, pre-assigned peer IDs (assigned_peer_id), per-peer role tracking (peerRoles), and the 12-second mobile background freeze grace window (pendingDisconnectTimers).

However, zero_client/src/ still carries ~2,500 lines of obsolete pre-ticket-032 scaffolding (ui.rs, bootstrap.rs, and legacy unintegrated DOM/WebRTC glue in webrtc.rs and mesh.rs). Specifically:

zero_client/src/ui.rs contains a hardcoded, outdated HTML string template (ChatUi::mount) and dead DOM bindings that were superseded by zero_client/index.html.
zero_client/src/bootstrap.rs contains legacy DOM initialization routines that are never invoked by the production application.
zero_client/src/webrtc.rs contains unconfigured web-sys RTC stubs lacking STUN/TURN servers and smart ICE candidate gathering.

ticket-048 consolidates zero_client into a lean, purpose-built WASM Cryptography, Protocol & Pure-State library—pruning dead DOM/WebRTC glue, upgrading the pure-Rust state machine (mesh.rs) to reflect the battle-tested ticket-045 resilience invariants (peerRoles and disconnect grace periods), and shrinking the compiled .wasm binary footprint.


2. Target File Scope
zero_client/src/lib.rs (Remove exports of dead modules ui and bootstrap; expose a clean, documented public surface centered on WasmCryptoSession, crypto, invite, and mesh)
zero_client/src/ui.rs & zero_client/src/bootstrap.rs (Remove dead legacy DOM scaffolding)
zero_client/src/webrtc.rs (Prune unused web-sys RtcPeerConnection DOM glue while retaining pure-Rust PeerLifecycleState / SdpValidator types used by mesh.rs, or fold those pure state types cleanly into mesh.rs)
zero_client/src/mesh.rs & zero_client/tests/mesh_simulation_test.rs (Align MeshTopology with the ticket-045/047 invariants: per-peer role tracking PeerRole::{Inviter, Joiner, Mesh}, signed PeerJoined introductions, and two-stage transient disconnect grace handling begin_disconnect_grace / recover_peer / expire_disconnect_grace)
zero_client/Cargo.toml (Prune unused web-sys DOM element feature flags such as HtmlButtonElement, HtmlInputElement, HtmlDivElement, Document, Window, Element if no longer needed, reducing compile time and WASM binary size)
openspec/specs/overview.md (Update the architecture overview to accurately document the production split between WasmCryptoSession in Rust/WASM and the browser runtime in index.html)


3. Detailed Technical Specification
Part A: Prune Obsolete DOM & WebRTC Scaffolding (ui.rs, bootstrap.rs, webrtc.rs)
Remove zero_client/src/ui.rs and zero_client/src/bootstrap.rs:
Delete zero_client/src/ui.rs and zero_client/src/bootstrap.rs (and any dedicated unit test files that exclusively test ChatUi::mount or bootstrap).
Remove pub mod ui; and pub mod bootstrap; from zero_client/src/lib.rs.
Replace Heavy web-sys WebRTC Glue with Pure State Types:
Inspect zero_client/src/webrtc.rs. Keep pure-Rust validation and state primitives (PeerLifecycleState, SdpType, SDP validation helpers) that are used by mesh.rs and native tests, and remove the unused #[cfg(target_arch = "wasm32")] web-sys RtcPeerConnection / RtcDataChannel closures (since ZeroWebRtcEngine in index.html owns the browser RTCPeerConnection lifecycle with smart srflx ICE gathering and mobile visibilitychange reconnection).
Slim Down zero_client/Cargo.toml:
Remove unused web-sys DOM feature flags (Document, Element, HtmlElement, HtmlInputElement, HtmlButtonElement, Window, RtcPeerConnection, RtcDataChannel, etc.) so zero_client compiles faster and produces a smaller zero_client_bg.wasm binary.


Part B: Align zero_client/src/mesh.rs with ticket-045 & ticket-047 Invariants
Update MeshTopology in zero_client/src/mesh.rs so that the Rust mesh state machine matches the exact production rules proved out in ticket-045 and ticket-047:

Per-Peer Role Tracking (PeerRole):

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]

pub enum PeerRole {

    Inviter,

    Joiner,

    Mesh,

}

Track peer_roles: HashMap<String, PeerRole> inside MeshTopology.
Provide set_peer_role(&mut self, peer_id: &str, role: PeerRole) and peer_role(&self, peer_id: &str) -> Option<PeerRole>.
Transient Disconnect Grace State (DisconnectGraceState):
Instead of PeerLifecycleState::Closed immediately purging a peer on an unannounced drop, support:
mark_peer_disconnected_grace(&mut self, peer_id: &str, timestamp_ms: u64) — marks the peer as in grace period without emitting on_burn_peer_messages or on_peer_left.
recover_peer(&mut self, peer_id: &str) — cancels any pending grace state and removes peer_id from departed_peers if they re-open within the grace window.
expire_peer_grace(&mut self, peer_id: &str) — finalizes departure and triggers selective burn + departure notice once the grace window elapses without recovery.
Explicit PeerLeftAndBurn continues to bypass the grace window and burn immediately.


Part C: Refresh openspec/specs/overview.md
Update openspec/specs/overview.md so future agents and contributors have an accurate map of the repository:

Document the Option B Modernized Burner Keybundle (Ed25519 signing + X25519 DH + ChaCha20-Poly1305 AEAD + ZeroizeOnDrop).
Document the Blind Dead-Drop Handshake (sealed_sdp_answer encrypted to host_public_key before traversing zero_network).
Document the clean boundary between WasmCryptoSession (zero_client/src/crypto.rs) and zero_client/index.html.


4. Verification & Acceptance Criteria
Gate 1: Rust Workspace & Clippy Gate
cargo test --workspace — All unit and integration tests across zero_protocol, zero_client, and zero_network must pass with 0 failures.
cargo clippy --workspace --all-targets -- -D warnings — Must pass with zero warnings (no dead code, unused imports, or orphaned modules).
Gate 2: WASM Build & Bundle Size Verification
./scripts/deploy_dry_run.sh — Must compile zero_client to wasm32-unknown-unknown, generate ./dist/pkg/zero_client_bg.wasm and ./dist/pkg/zero_client.js, and stage index.html and style.css cleanly.
Verify WasmCryptoSession exports remain 100% intact for zero_client/index.html.
Gate 3: End-to-End Browser Regression Suite
Run node scripts/test_mesh_scenario.js (all 22 headless browser tests including Test 22 Scenarios A–E) and verify 100% pass rate with zero regressions.
