# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-002"
title: "WebRTC PeerConnection & DataChannel WASM Engine"
state: "PLANNING"
roadmap: "serverless-p2p-chat"
depends_on: ["ticket-001"]
created_at: "2026-10-16T21:28:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-002/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/serverless-p2p-chat.md`

## 1. Problem Statement & Objective
In order to achieve serverless browser-to-browser communication, we must implement a pure Rust WebAssembly engine (`zero_client`) that controls browser WebRTC primitives via `web-sys`. This engine manages the lifecycle of `RtcPeerConnection` and `RtcDataChannel`, handles SDP offer/answer generation and ICE candidate gathering, and provides an event-driven callback interface for sending and receiving binary and text messages. A local test harness allows validating bidirectional data transfer manually via copy-paste SDP.

## 2. Scope Boundaries
- **In-Scope**:
  - `zero_client/Cargo.toml` configured for `wasm32-unknown-unknown` with `wasm-bindgen`, `web-sys` (features: `RtcPeerConnection`, `RtcDataChannel`, `RtcSessionDescriptionInit`, `RtcIceCandidateInit`, `RtcConfiguration`, etc.), and `zero_protocol`.
  - Rust WebRTC connection manager (`webrtc.rs`) managing peer lifecycle (Connecting, Open, Closing, Closed).
  - SDP Offer generation with bundled ICE candidates (Trickle ICE disabled or buffered until candidate gathering finishes).
  - Remote SDP Answer ingestion and DataChannel event dispatching.
  - Test harness (`zero_client/test_harness.html` and lightweight JS runner) allowing manual exchange of SDP strings between two browser tabs on `localhost` to confirm bidirectional DataChannel flow.
- **Out-of-Scope**:
  - Cloudflare Worker dead-drop signaling (handled by `ticket-003`).
  - Automated URL `#fragment` parsing (deferred to `ticket-004`).
  - Multi-peer gossip mesh routing (deferred to `ticket-006`).

## 3. Requirements & Acceptance Criteria
- [ ] **REQ-002.1**: `zero_client` compiles cleanly to `wasm32-unknown-unknown` via `cargo build -p zero_client --target wasm32-unknown-unknown` (or native test stub).
  - *Verification*: Target build succeeds without compiler errors.
- [ ] **REQ-002.2**: `create_offer()` initiates an `RtcPeerConnection`, creates an ordered, reliable `RTCDataChannel` labeled `"zero-chat"`, and gathers local ICE candidates into a self-contained SDP string.
  - *Verification*: Generated SDP payload contains both offer description and candidate lines.
- [ ] **REQ-002.3**: `accept_offer_and_create_answer()` accepts an incoming SDP Offer string, creates an answer with candidates, and resolves an answer SDP string.
  - *Verification*: Remote description is successfully applied without throwing DOMException.
- [ ] **REQ-002.4**: Bidirectional data exchange over `RTCDataChannel` triggers message callbacks for UTF-8 and binary packets.
  - *Verification*: Manual localhost verification harness confirms sending `"hello from Alice"` receives `"hello from Bob"` over direct P2P.

## 4. Architectural Design & Impacted Modules
- **New Modules / Files**:
  - `zero_client/Cargo.toml`
  - `zero_client/src/lib.rs`
  - `zero_client/src/webrtc.rs`
  - `zero_client/test_harness.html`
  - `zero_client/tests/webrtc_tests.rs`
- **Modified Modules**:
  - `Cargo.toml` (workspace members)

## 5. Decomposition Plan (Architect Briefs)

| Batch | Brief ID | Role | Target Thinking Level | Description | Files Scope | Dependencies |
| :---: | :--- | :--- | :---: | :--- | :--- | :--- |
| **1** | `brief-01-webrtc-lifecycle` | Implementer | `HIGH` | Rust WASM WebRTC lifecycle, PeerConnection, and DataChannel management | `zero_client/Cargo.toml`, `zero_client/src/webrtc.rs`, `zero_client/src/lib.rs` | None |
| **1** | `brief-02-test-harness` | Implementer | `LOW` | Minimalist localhost SDP test harness HTML & JS glue | `zero_client/test_harness.html` | None |
| **2** | `brief-val-webrtc` | Validator | `MEDIUM` | Unit tests & compilation verification for WASM target | `zero_client/tests/*` | Batch 1 |
| **2** | `brief-val-lint` | Validator | `LOW` | Clippy and formatting checks across `zero_client` | `zero_client/*` | Batch 1 |
| **3** | `brief-rev` | Reviewer | `HIGH` | Architectural audit, memory leak avoidance, and WebRTC state machine review | Candidate diff | Batch 2 |
