# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-003"
title: "Cloudflare Worker Hibernating Durable Object Dead-Drop"
state: "PLANNING"
roadmap: "serverless-p2p-chat"
depends_on: ["ticket-001"]
created_at: "2026-10-16T21:28:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-003/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/serverless-p2p-chat.md`

## 1. Problem Statement & Objective
To enable an ephemeral, zero-cost initial connection handshake without running persistent signaling infrastructure, we must implement a Rust Cloudflare Worker utilizing a Durable Object with WebSocket Hibernation (`zero_network`). The host connects over WebSocket and the DO sleeps with zero duration billing. Joining peers drop their encrypted SDP Answer via an HTTP `POST` route. The DO awakens for ~3 milliseconds, validates the HMAC-SHA256 token against the host's secret saved via `serialize_attachment()`, burns the single-use invite UUID, pipes the answer down the host's WebSocket, and immediately returns to hibernation.

## 2. Scope Boundaries
- **In-Scope**:
  - `zero_network/Cargo.toml` configured with `worker` (`workers-rs`) and `zero_protocol`.
  - `zero_network/wrangler.toml` configuring the Worker and Durable Object bindings.
  - Durable Object class with WebSocket Hibernation API:
    - Host endpoint (`/host/:do_id`): Upgrades to WebSocket, extracts 32-byte `host_secret` from header/auth query, attaches it to the socket via `serialize_attachment()`, and hibernates.
    - Dead-drop endpoint (`POST /drop/:do_id/:invite_uuid`): Extracts bearer verification token, checks HMAC match against the host's secret, verifies the UUID is not in `burned_uuids`, appends the UUID to the burned set, forwards the encrypted SDP Answer down the host's WebSocket, and returns 200 OK.
  - Ephemeral in-memory burn tracking (purged automatically when DO memory unloads; zero persistent DB storage).
  - Unit and integration tests for route handlers and HMAC validation logic.
- **Out-of-Scope**:
  - Persistent database or KV storage (in-memory DO state only).
  - Client-side DOM / WebRTC logic (handled in `ticket-002` and `ticket-004`).

## 3. Requirements & Acceptance Criteria
- [ ] **REQ-003.1**: `zero_network` compiles with `workers-rs` target.
  - *Verification*: `cargo check -p zero_network` compiles without error.
- [ ] **REQ-003.2**: WebSocket upgrade on `/host/:do_id` attaches the `host_secret` and enables DO hibernation.
  - *Verification*: Handler accepts valid 32-byte hex or base64 secret and returns 101 Switching Protocols.
- [ ] **REQ-003.3**: Invalid or tampered tokens on `POST /drop/:do_id/:invite_uuid` are rejected with HTTP 403 Forbidden.
  - *Verification*: Tests verify that mismatched HMAC signatures fail validation.
- [ ] **REQ-003.4**: Replayed invite UUIDs are rejected with HTTP 410 Gone / 409 Conflict.
  - *Verification*: Repeated drop submission with an already-burned UUID fails on second attempt.
- [ ] **REQ-003.5**: Valid drop payload is piped directly to the host's hibernating WebSocket.
  - *Verification*: Mock test confirms WebSocket receives the serialized answer payload.

## 4. Architectural Design & Impacted Modules
- **New Modules / Files**:
  - `zero_network/Cargo.toml`
  - `zero_network/wrangler.toml`
  - `zero_network/src/lib.rs`
  - `zero_network/src/durable_object.rs`
  - `zero_network/src/handlers.rs`
  - `zero_network/tests/network_tests.rs`
- **Modified Modules**:
  - `Cargo.toml` (workspace members)

## 5. Decomposition Plan (Architect Briefs)

| Batch | Brief ID | Role | Target Thinking Level | Description | Files Scope | Dependencies |
| :---: | :--- | :--- | :---: | :--- | :--- | :--- |
| **1** | `brief-01-worker-do-hibernation` | Implementer | `HIGH` | Durable Object WebSocket hibernation and attachment serialization | `zero_network/Cargo.toml`, `zero_network/wrangler.toml`, `zero_network/src/durable_object.rs`, `zero_network/src/lib.rs` | None |
| **1** | `brief-02-dead-drop-route` | Implementer | `MEDIUM` | Dead-drop HTTP POST endpoint, HMAC validation, and invite burning | `zero_network/src/handlers.rs` | None |
| **2** | `brief-val-network` | Validator | `MEDIUM` | Automated testing for HMAC checks and drop forwarding | `zero_network/tests/*` | Batch 1 |
| **2** | `brief-val-lint` | Validator | `LOW` | Clippy and formatting audit across `zero_network` | `zero_network/*` | Batch 1 |
| **3** | `brief-rev` | Reviewer | `HIGH` | Security audit (HMAC timing resistance, DoS resilience, zero-storage invariant) | Candidate diff | Batch 2 |
