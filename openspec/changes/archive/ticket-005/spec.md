# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-005"
title: "Ephemeral Chat UI, Cryptographic Payload Sealing & E2E Validation"
state: "PLANNING"
roadmap: "serverless-p2p-chat"
depends_on: ["ticket-004"]
created_at: "2026-10-16T21:28:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-005/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/serverless-p2p-chat.md`

## 1. Problem Statement & Objective
Once direct WebRTC DataChannels are established, we must provide users with an intuitive, aesthetically rich, and strictly zero-footprint real-time chat interface. All message payloads flowing across the DataChannel must be cryptographically sealed using ChaCha20-Poly1305 with keys derived from the initial X25519 key exchange. The application enforces zero client-side persistence: closing the tab permanently erases all session keys, message history, and DOM elements, leaving zero forensic trace. Comprehensive end-to-end integration tests validate the entire flow.

## 2. Scope Boundaries
- **In-Scope**:
  - `zero_client/src/ui.rs`: Responsive web chat interface manipulating the DOM via `web-sys` (status header showing P2P snap, encrypted chat feed, auto-expanding message input, copyable invite link button).
  - Styling in `zero_client/style.css`: Clean, modern dark mode / minimalist aesthetic (glassmorphic cards, subtle micro-animations, clear cryptographic status indicators).
  - `zero_client/src/crypto.rs`: Per-message symmetric payload sealing and authentication (X25519 shared secret derivation + ChaCha20-Poly1305 AEAD).
  - Memory cleanliness audit: Zero usage of `localStorage`, `sessionStorage`, `indexedDB`, or service worker caches.
  - End-to-end integration tests (`tests/e2e_handshake.rs`): Automated simulation validating invite generation, mock dead-drop, P2P channel snap, and authenticated message roundtrip.
- **Out-of-Scope**:
  - Multi-peer gossip routing (deferred to `ticket-006`).
  - Native mobile app wrappers (web browser target only).

## 3. Requirements & Acceptance Criteria
- [ ] **REQ-005.1**: Ephemeral chat interface renders connection status, invite creation modal/button, and real-time message stream.
  - *Verification*: UI renders cleanly without JavaScript console errors in modern browsers.
- [ ] **REQ-005.2**: Outgoing chat messages are encrypted with ChaCha20-Poly1305 before transmission over `RTCDataChannel`.
  - *Verification*: Raw packet inspection confirms payload is high-entropy ciphertext with accompanying 12-byte nonce and 16-byte tag.
- [ ] **REQ-005.3**: Incoming packets are authenticated and decrypted; tampered or unauthenticated packets are dropped with a visual alert.
  - *Verification*: Manipulated ciphertexts trigger authentication failure without crashing the client.
- [ ] **REQ-005.4**: Zero persistence invariant is verified.
  - *Verification*: Automated audit confirms `localStorage`, `sessionStorage`, and `indexedDB` remain empty throughout entire chat lifecycle.
- [ ] **REQ-005.5**: Automated end-to-end integration test passes in CI/test runner.
  - *Verification*: `cargo test --test e2e_handshake` completes successfully.

## 4. Architectural Design & Impacted Modules
- **New Modules / Files**:
  - `zero_client/src/ui.rs`
  - `zero_client/src/crypto.rs`
  - `zero_client/index.html`
  - `zero_client/style.css`
  - `tests/e2e_handshake.rs`
- **Modified Modules**:
  - `zero_client/src/lib.rs`
  - `zero_client/Cargo.toml`

## 5. Decomposition Plan (Architect Briefs)

| Batch | Brief ID | Role | Target Thinking Level | Description | Files Scope | Dependencies |
| :---: | :--- | :--- | :---: | :--- | :--- | :--- |
| **1** | `brief-01-payload-sealing` | Implementer | `MEDIUM` | In-band X25519 key derivation and ChaCha20-Poly1305 packet sealing | `zero_client/src/crypto.rs`, `zero_client/src/lib.rs` | None |
| **1** | `brief-02-ephemeral-ui` | Implementer | `MEDIUM` | Ephemeral DOM chat interface, glassmorphic styles, and event binding | `zero_client/src/ui.rs`, `zero_client/index.html`, `zero_client/style.css` | None |
| **2** | `brief-val-e2e` | Validator | `MEDIUM` | End-to-end integration test harness and zero-persistence verification | `tests/e2e_handshake.rs` | Batch 1 |
| **2** | `brief-val-lint` | Validator | `LOW` | Clippy, format, and static DOM security audit | Full repository | Batch 1 |
| **3** | `brief-rev` | Reviewer | `HIGH` | Cryptographic audit, zero-footprint verification, and UX polish review | Candidate diff | Batch 2 |
