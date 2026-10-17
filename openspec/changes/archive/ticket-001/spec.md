# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-001"
title: "Workspace Architecture, Protocol Envelope & Crypto Primitives"
state: "PLANNING"
roadmap: "serverless-p2p-chat"
depends_on: []
created_at: "2026-10-16T21:28:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-001/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/serverless-p2p-chat.md`

## 1. Problem Statement & Objective
To enable coordinated, type-safe development across the client (`zero_client`) and Cloudflare Worker (`zero_network`), we must establish the root Cargo workspace and a shared protocol crate (`crates/zero_protocol`). This shared crate defines all serialization envelopes, DTOs, error schemas, and core cryptographic primitives (HMAC-SHA256, X25519 key exchange, ChaCha20-Poly1305 symmetric sealing) with target-independent compatibility (`wasm32-unknown-unknown` and native).

## 2. Scope Boundaries
- **In-Scope**:
  - Top-level `Cargo.toml` establishing a multi-crate workspace (`zero_client`, `zero_network`, `crates/zero_protocol`).
  - `crates/zero_protocol`: Shared message data transfer objects (`SdpOffer`, `SdpAnswer`, `InviteToken`, `DropPayload`, `ChatMessage`, `MeshRelayMessage`).
  - Cryptographic primitive modules: `hmac_sha256` token computation/verification, `x25519` keypair generation, and `chacha20poly1305` AEAD encryption/decryption helpers.
  - Serialization helpers with `serde` and `serde_json`.
  - Comprehensive unit test suite running natively via `cargo test`.
- **Out-of-Scope**:
  - WebRTC browser APIs and DOM manipulation (deferred to `ticket-002`).
  - Cloudflare Worker runtime handlers (deferred to `ticket-003`).

## 3. Requirements & Acceptance Criteria
- [ ] **REQ-001.1**: Root Cargo workspace is cleanly configured and `cargo check --workspace` succeeds.
  - *Verification*: `cargo check --workspace` compiles cleanly without missing crate errors.
- [ ] **REQ-001.2**: Data transfer objects (DTOs) for signaling envelopes are defined with Serde `Serialize` and `Deserialize`.
  - *Verification*: Unit tests verify bidirectional JSON serialization of `InvitePayload`, `DropPayload`, and `ChatMessage`.
- [ ] **REQ-001.3**: Cryptographic token generation computes `HMAC_SHA256(host_secret, invite_uuid)` identically across native and WASM targets.
  - *Verification*: Unit tests assert HMAC generation and constant-time verification against known test vectors.
- [ ] **REQ-001.4**: Symmetric payload sealing with `ChaCha20Poly1305` produces verifiable ciphertexts with ephemeral nonces.
  - *Verification*: Roundtrip encrypt-then-decrypt test confirms plaintext integrity and tampering detection.

## 4. Architectural Design & Impacted Modules
- **New Modules / Files**:
  - `Cargo.toml` (workspace manifest)
  - `crates/zero_protocol/Cargo.toml`
  - `crates/zero_protocol/src/lib.rs`
  - `crates/zero_protocol/src/crypto.rs`
  - `crates/zero_protocol/src/envelope.rs`
  - `crates/zero_protocol/tests/protocol_tests.rs`
- **Modified Modules**:
  - `.gitignore` (ensure target directories and test artifacts are excluded)
  - `factory.yaml`

## 5. Decomposition Plan (Architect Briefs)

| Batch | Brief ID | Role | Target Thinking Level | Description | Files Scope | Dependencies |
| :---: | :--- | :--- | :---: | :--- | :--- | :--- |
| **1** | `brief-01-workspace-schemas` | Implementer | `LOW` | Workspace manifest and shared DTO envelope schemas | `Cargo.toml`, `crates/zero_protocol/Cargo.toml`, `crates/zero_protocol/src/envelope.rs`, `crates/zero_protocol/src/lib.rs` | None (Parallel) |
| **1** | `brief-02-crypto-primitives` | Implementer | `LOW` | Cryptographic primitives (HMAC-SHA256, X25519, ChaCha20-Poly1305) | `crates/zero_protocol/src/crypto.rs` | None (Parallel) |
| **2** | `brief-val-tests` | Validator | `LOW` | Protocol serialization and crypto test suite | `crates/zero_protocol/tests/*` | Batch 1 |
| **2** | `brief-val-lint` | Validator | `LOW` | Workspace clippy and format audit | Full repository | Batch 1 |
| **3** | `brief-rev` | Reviewer | `HIGH` | Architectural audit and cryptographic soundness review | Candidate diff | Batch 2 |
