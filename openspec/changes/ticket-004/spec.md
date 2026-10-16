# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-004"
title: "Stateless HMAC Invite Generator & URL Fragment Auto-Drop Joiner"
state: "PLANNING"
roadmap: "serverless-p2p-chat"
depends_on: ["ticket-002", "ticket-003"]
created_at: "2026-10-16T21:28:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-004/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/serverless-p2p-chat.md`

## 1. Problem Statement & Objective
Wire the end-to-end Pattern B Stateless HMAC handshake into the client application. The host's client generates a single-use guest keypair, an `Invite_UUID`, and computes the verification token `Token = HMAC_SHA256(host_secret, Invite_UUID)`. It bundles the SDP Offer, `Invite_UUID`, `Token`, and guest public key into a URL `#fragment` link. When a joiner clicks the link, the joiner's browser automatically reads the `#fragment` (which never reaches any server), generates its SDP Answer, signs it, and dispatches a background HTTP `POST` to the dead-drop endpoint `/drop/{DO_ID}/{Invite_UUID}`. The host receives the answer via WebSocket, and the WebRTC DataChannel immediately establishes directly between peers without user intervention.

## 2. Scope Boundaries
- **In-Scope**:
  - `zero_client/src/invite.rs`: Offline invite generation, Base64URL fragment packing and unpacking.
  - `zero_client/src/bootstrap.rs`: Joiner auto-detection of URL `#fragment`, background fetch to `/drop/:do_id/:invite_uuid` using `web-sys::Window::fetch_with_request`, and automated WebRTC answering.
  - Host WebSocket connection manager to `wss://edge.app/host/{DO_ID}` for receiving joiner answers and completing the WebRTC peer connection.
  - Ephemeral state transition: once the WebRTC DataChannel transitions to `Open`, the signaling WebSocket is immediately closed, severing all edge connections.
  - Browser-level tests and mock integration tests for the fragment parsing and dispatch pipeline.
- **Out-of-Scope**:
  - Final chat UI layout (handled in `ticket-005`).
  - Gossip mesh forwarding between multiple peers (deferred to `ticket-006`).

## 3. Requirements & Acceptance Criteria
- [ ] **REQ-004.1**: Host creates offline invite bundle and produces a valid shareable URL `#fragment`.
  - *Verification*: Generated URL contains `#payload=` with valid Base64URL-encoded JSON holding `invite_uuid`, `token`, `sdp_offer`, and guest public key.
- [ ] **REQ-004.2**: Client parses `#fragment` on page load without sending hash parameters in HTTP request headers.
  - *Verification*: Unit tests verify `window.location().hash()` extraction and decoding.
- [ ] **REQ-004.3**: Joiner client automatically creates an SDP Answer and dispatches the background HTTP `POST` to the dead-drop endpoint.
  - *Verification*: Drop request is fired with correct bearer token and JSON payload.
- [ ] **REQ-004.4**: Host receives the dropped answer over the hibernating WebSocket and completes the WebRTC handshake.
  - *Verification*: DataChannel state transitions to `Open`.
- [ ] **REQ-004.5**: Signaling WebSocket on host is closed immediately upon DataChannel `Open` confirmation.
  - *Verification*: Zero active edge sockets remain after P2P channel establishment.

## 4. Architectural Design & Impacted Modules
- **New Modules / Files**:
  - `zero_client/src/invite.rs`
  - `zero_client/src/bootstrap.rs`
  - `zero_client/tests/invite_tests.rs`
- **Modified Modules**:
  - `zero_client/src/lib.rs`
  - `zero_client/src/webrtc.rs`

## 5. Decomposition Plan (Architect Briefs)

| Batch | Brief ID | Role | Target Thinking Level | Description | Files Scope | Dependencies |
| :---: | :--- | :--- | :---: | :--- | :--- | :--- |
| **1** | `brief-01-invite-generator` | Implementer | `MEDIUM` | Offline invite generation, URL hash formatting, and token calculation | `zero_client/src/invite.rs`, `zero_client/src/lib.rs` | None |
| **1** | `brief-02-joiner-autodrop` | Implementer | `MEDIUM` | Hash detection, background HTTP drop fetch, and answer dispatch | `zero_client/src/bootstrap.rs`, `zero_client/src/webrtc.rs` | None |
| **2** | `brief-val-bootstrap` | Validator | `LOW` | End-to-end token verification and fragment parsing tests | `zero_client/tests/*` | Batch 1 |
| **2** | `brief-val-lint` | Validator | `LOW` | Linting and code formatting audit | `zero_client/*` | Batch 1 |
| **3** | `brief-rev` | Reviewer | `HIGH` | Architectural audit for edge severance and zero-footprint lifecycle | Candidate diff | Batch 2 |
