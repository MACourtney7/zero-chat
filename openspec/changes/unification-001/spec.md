# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "unification-001"
title: "Bootstrap Sibling Repositories (zero_protocol & zero_network) and Re-Wire zero_chat Baseline"
state: "IN_PROGRESS"
state: "COMPLETED"
created_at: "2026-11-01T15:27:00Z"
base_branch: "main"
ticket_branch: "factory/unification-001/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

Currently, `zero_chat` hosts the entire codebase as a single monolithic repository containing:
1. `crates/zero_protocol`: Wire types, envelopes, and cryptographic primitives.
2. `zero_network`: Ephemeral Cloudflare signaling worker and hibernating Durable Object.
3. `zero_client`: Headless Wasm P2P mesh engine, cryptographic session bridge, and HTML/CSS UI application.

To enable multi-application peer-to-peer interoperability across future projects (such as `virtual_dog_walker`), the core protocol engine and signaling worker must be factored out into independent sibling repositories at the exact same parent directory level (`../` relative to `zero_chat`).

Additionally, the existing sibling directory `../virtual-dog-walker` must be normalized to snake_case (`../virtual_dog_walker`).

### Target Directory Topology (Relative to Parent Directory `../`)
```text
<parent_dir>/
├── zero_protocol/          # NEW REPO: Cargo workspace containing `zero_protocol` + `zero_mesh`
│   ├── Cargo.toml
│   └── crates/
│       ├── zero_protocol/  # Pure Rust wire types & crypto (X25519, Ed25519, ChaCha20-Poly1305, HMAC)
│       └── zero_mesh/      # Headless Wasm P2P client (WebRTC, ICE, Signaling, MeshTopology, WasmCryptoSession)
├── zero_network/           # NEW REPO: Standalone Cloudflare Signaling Worker & Hibernating Durable Object
│   ├── Cargo.toml
│   ├── wrangler.toml
│   └── src/
│       ├── lib.rs
│       ├── durable_object.rs
│       ├── auth.rs
│       ├── handlers.rs
│       └── validation.rs
├── zero_chat/              # EXISTING REPO: Slimmed down to Chat Frontend + UI bindings consuming sibling crates
│   ├── Cargo.toml
│   ├── zero_client/        # Consumes zero_protocol & zero_mesh, compiles wasm bundle for zero-chat.pages.dev
│   └── dist/
└── virtual_dog_walker/     # RENAMED from `virtual-dog-walker`
```

---

## 2. Requirements & Acceptance Criteria

### REQ-UNI.1: Resolve Parent Directory & Bootstrap Sibling Repositories
- Detect parent directory of `zero_chat`.
- Create `../zero_protocol` and `../zero_network` directories and initialize each as a git repository (`git init`).
- If `../virtual-dog-walker` exists and `../virtual_dog_walker` does not exist, rename `../virtual-dog-walker` to `../virtual_dog_walker`.

### REQ-UNI.2: Populate `../zero_protocol` Cargo Workspace (`zero_protocol` + `zero_mesh`)
- Create workspace `../zero_protocol/Cargo.toml` containing members `["crates/zero_protocol", "crates/zero_mesh"]` with `resolver = "2"`.
- Migrate `zero_chat/crates/zero_protocol` into `../zero_protocol/crates/zero_protocol`:
  - Ensure zero `web-sys` or `worker` dependencies so it compiles across native, Wasm, and workerd.
  - Verify all unit and cryptographic tests pass 100% via `cargo test -p zero_protocol`.
- Extract headless `crates/zero_mesh` from `zero_chat/zero_client`:
  - Create `../zero_protocol/crates/zero_mesh` (`Cargo.toml`, `src/lib.rs`, `src/crypto.rs`, `src/invite.rs`, `src/mesh.rs`, `src/webrtc.rs`).
  - Decouple from any DOM/UI concerns: ensure zero references to HTML DOM IDs or CSS classes.
  - Expose headless event hooks/callbacks (e.g. `MeshEventHandler` or callback closures) for state changes and message handling.
  - Verify `cargo test -p zero_mesh` and `cargo check -p zero_mesh --target wasm32-unknown-unknown` pass.
- Commit initial baseline in `../zero_protocol`.

### REQ-UNI.3: Populate `../zero_network` Standalone Worker Repository
- Copy `zero_chat/zero_network/` files (`Cargo.toml`, `wrangler.toml`, `src/lib.rs`, `src/durable_object.rs`, `src/handlers.rs`) into `../zero_network/`.
- Update `../zero_network/Cargo.toml` to depend on sibling `zero_protocol`:
  ```toml
  [dependencies]
  zero_protocol = { path = "../zero_protocol/crates/zero_protocol" }
  ```
- Run `cargo test` and `cargo check --target wasm32-unknown-unknown` in `../zero_network` to confirm all tests and worker compilation pass.
- Commit initial baseline in `../zero_network`.

### REQ-UNI.4: Slim Down `zero_chat` & Re-Wire Baseline
- Update `zero_chat/Cargo.toml` workspace members: remove `crates/zero_protocol` and `zero_network`.
- Delete extracted directories `crates/zero_protocol` and `zero_network` from `zero_chat`.
- Update `zero_chat/zero_client/Cargo.toml` to depend on sibling crates:
  ```toml
  [dependencies]
  zero_protocol = { path = "../../zero_protocol/crates/zero_protocol" }
  zero_mesh = { path = "../../zero_protocol/crates/zero_mesh" }
  ```
- Re-export `WasmCryptoSession` and required types from `zero_mesh` in `zero_chat/zero_client/src/lib.rs`, preserving exact `#[wasm_bindgen]` entrypoints expected by `zero_client/index.html` (`dist/pkg/zero_client.js`).
- Update `scripts/deploy_dry_run.sh` to remove obsolete `zero_network` check.
- Verify `cargo test --workspace` passes in `zero_chat`.
- Verify `bash zero_client/tests/browser/run_test.sh` passes all 24 browser test scenarios.
- Verify `./scripts/deploy_dry_run.sh` produces verified release WASM and web assets.

---

## 3. Blast Radius & Affected Modules

- `../zero_protocol/` (New workspace repository)
- `../zero_network/` (New worker repository)
- `../virtual_dog_walker/` (Renamed from `virtual-dog-walker`)
- `zero_chat/Cargo.toml` (Workspace definition)
- `zero_chat/crates/zero_protocol/` (Deleted / moved)
- `zero_chat/zero_network/` (Deleted / moved)
- `zero_chat/zero_client/Cargo.toml` (Dependency paths)
- `zero_chat/zero_client/src/lib.rs` (Re-exports from `zero_mesh`)
- `zero_chat/scripts/deploy_dry_run.sh` (Removal of local `zero_network` check)

---

## 4. Work Breakdown (Brief Decomposition)

- **Brief 1 (Sibling Bootstrap & Extraction)**:
  - Initialize `../zero_protocol` workspace with `crates/zero_protocol` and `crates/zero_mesh`.
  - Initialize `../zero_network` with updated sibling dependency.
  - Rename `../virtual-dog-walker` to `../virtual_dog_walker`.
- **Brief 2 (zero_chat Re-wiring & Pruning)**:
  - Remove extracted crates from `zero_chat/Cargo.toml` and filesystem.
  - Wire `zero_chat/zero_client` to `../zero_protocol/crates/*`.
  - Update `scripts/deploy_dry_run.sh`.
- **Brief 3 (Validation)**:
  - Run `cargo test` across `../zero_protocol`, `../zero_network`, and `zero_chat`.
  - Run all 24 browser headless test scenarios in `zero_chat`.
- **Brief 4 (Review)**:
  - Independent architectural review of sibling repository topologies, boundary decoupling, and zero regressions.

---

## 5. Verification Plan

1. In `../zero_protocol`:
   - `cargo test --workspace` (All protocol and mesh tests pass).
   - `cargo check --target wasm32-unknown-unknown` passes.
2. In `../zero_network`:
   - `cargo test` passes against sibling `zero_protocol`.
   - `cargo check --target wasm32-unknown-unknown` passes.
3. In `zero_chat`:
   - `cargo test --workspace` passes cleanly.
   - `cargo clippy --workspace --all-targets -- -D warnings` passes with 0 warnings.
   - `cargo fmt -- --check` passes.
   - `bash zero_client/tests/browser/run_test.sh` passes all 24 headless browser tests.
   - `./scripts/deploy_dry_run.sh` builds release WASM and verifies bundle.

