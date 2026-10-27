# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-051"
title: "v1.0 Release Polish — WASM Binary Optimization (wasm-opt -Oz) & Root README.md Showcase"
state: "IN_PROGRESS"
created_at: "2026-10-27T23:14:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-051/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

With `ticket-045` through `ticket-050` completed and verified in production, ZeroChat's core architecture, Option B modernized cryptography (X25519 + ChaCha20-Poly1305 + Ed25519 + ZeroizeOnDrop), resilient multi-peer mesh state machine, and mobile Cloudflare Realtime TURN CGNAT traversal are 100% operational.

`ticket-051` closes out the **v1.0 Production Milestone** with two release-engineering deliverables:
1. **WASM Size & Release Profile Optimization**:
   Configure the workspace root `Cargo.toml` with aggressive size-stripping flags (`opt-level = "z"`, `lto = true`, `codegen-units = 1`, `panic = "abort"`, `strip = true`) and integrate an automatic post-bindgen `wasm-opt -Oz` pass into `scripts/deploy_dry_run.sh` (with graceful fallback if `wasm-opt` is not present on PATH), reporting before/after byte sizes.
2. **Production Architectural Showcase (`README.md`)**:
   Author a staff-level, comprehensive root `README.md` documenting the v1.0 architecture, the 3-crate workspace (`crates/zero_protocol`, `zero_network`, `zero_client`), the Option B Burner Keybundle, the Blind Dead-Drop handshake (`sealed_sdp_answer`), the self-healing multi-peer mesh state machine, the 3-tier Cloudflare Realtime TURN CGNAT relay, and local development and test commands.

---

## 2. Requirements & Acceptance Criteria

### REQ-051.1: Workspace `[profile.release]` Configuration
- In `Cargo.toml` (root workspace):
  - Add a dedicated `[profile.release]` section:
    ```toml
    [profile.release]
    opt-level = "z"
    lto = true
    codegen-units = 1
    panic = "abort"
    strip = true
    ```
  - Verify that `cargo test --workspace` and `cargo build -p zero_client --release --target wasm32-unknown-unknown` compile cleanly.

### REQ-051.2: Post-Bindgen `wasm-opt -Oz` Pass in `scripts/deploy_dry_run.sh`
- In `scripts/deploy_dry_run.sh`:
  - After `wasm-bindgen` generates `dist/pkg/zero_client_bg.wasm`:
    - Check if `dist/pkg/zero_client_bg.wasm` exists and record its size before optimization.
    - Check `if command -v wasm-opt >/dev/null 2>&1; then`:
      - Execute `wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int dist/pkg/zero_client_bg.wasm -o dist/pkg/zero_client_bg.wasm`.
      - Log the before and after byte sizes and percentage reduction.
    - If `wasm-opt` is not found, print a notice that `wasm-opt` was not found on PATH and continue with Cargo LTO/strip output.

### REQ-051.3: Root `README.md` Production Architectural Showcase
- Create/update `README.md` with:
  1. **Header & Badges**: ZeroChat (v1.0) — Ephemeral, Zero-Knowledge Peer-to-Peer Mesh Chat in Rust & WebAssembly.
  2. **System Architecture Overview (3-Crate Monorepo)**:
     - `crates/zero_protocol`: Wire envelopes, pairwise ChaCha20-Poly1305 AEAD, Ed25519 signatures, X25519 shared secret derivation, HMAC-SHA256 tokens.
     - `zero_network`: Cloudflare Worker & Hibernating SQLite Durable Objects (`DeadDropRoom`), single-use HMAC-authenticated dead-drops, and 3-tier Cloudflare Realtime TURN provisioning (`/ice`).
     - `zero_client`: Rust WebAssembly (`WasmCryptoSession`) + Vanilla JS WebRTC/DOM host, zero-persistence volatile memory wipe, mobile visualViewport layout.
  3. **Cryptographic Design (Option B Modernized Burner Keybundle)**:
     - In-memory `BurnerKeybundle` (`X25519Keypair` + `Ed25519Keypair`) protected by `Zeroize` and `ZeroizeOnDrop`.
     - Out-of-band `#payload=` URL fragment (Host public keys and HMAC token never travel over HTTP).
     - 100% Blind Dead-Drop (`sealed_sdp_answer`): Joiners seal SDP answer with host X25519 public key before POSTing; Worker relay is blind to WebRTC parameters and IP addresses.
     - Pairwise authenticated ChaCha20-Poly1305 AEAD (96-bit nonce, 128-bit MAC tag, 64-byte Ed25519 digital signature).
     - Deterministic memory wipe: `burn_all()` immediately scrubs secrets from WASM RAM on tab close or Leave.
  4. **Network & Mobile Resilience Engineering**:
     - Adaptive non-trickle ICE gathering (fast settle on broadband STUN, 350ms settle on TURN relay).
     - Mobile background freeze grace period (12s) distinguishing explicit departures from background tab suspension.
     - 3-tier Cloudflare Realtime TURN CGNAT traversal (`/ice`) resolving 4G/5G symmetric NAT and same-device cellular multi-tab hairpinning.
  5. **Local Development, Testing & Verification Guide**:
     - Exact commands for 117+ Rust tests (`cargo test --workspace`), clippy (`cargo clippy --workspace --all-targets -- -D warnings`), code format (`cargo fmt -- --check`), dry-run build (`./scripts/deploy_dry_run.sh`), and 23 browser E2E headless scenarios (`bash zero_client/tests/browser/run_test.sh`).

### REQ-051.4: Update Canonical System Specification
- Update `openspec/specs/overview.md` to reflect `v1.0.0` milestone completion, recording the release profile optimizations and v1.0.0 production status.

### REQ-051.5: Quality Gates & Verification
- `cargo test --workspace` (117+ tests pass, 0 fail).
- `cargo clippy --workspace --all-targets -- -D warnings` (0 warnings).
- `cargo fmt -- --check` (clean).
- `./scripts/deploy_dry_run.sh` (assembles static bundle in `dist/`, runs wasm-opt if available, and prints byte sizes).
- `bash zero_client/tests/browser/run_test.sh` (all 23 headless browser tests pass).

---

## 3. Architecture & Affected Modules

- `Cargo.toml`: Root workspace release profile optimization flags.
- `scripts/deploy_dry_run.sh`: Post-bindgen `wasm-opt -Oz` pass with size reporting.
- `README.md`: Root architectural showcase.
- `openspec/specs/overview.md`: Version and canonical spec update.
