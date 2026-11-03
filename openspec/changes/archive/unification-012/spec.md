---
ticket_id: "unification-012"
title: "Align wasm-bindgen-cli Version to 0.2.130 in CI Workflow"
state: "COMPLETED"
created_at: "2026-11-03T21:46:00Z"
target_repos:
  - "../virtual_dog_walker"
max_review_cycles: 3
---

# Change Proposal: unification-012 — Align wasm-bindgen-cli in CI Workflow

## 1. Problem Statement & Root Cause
In GitHub Actions run `41723946290` (and `41723871426`), the `test` job passed cleanly, but the `deploy` job failed during `npm run build`:
```text
=== Building zero_mesh WebAssembly Package ===
Building crate from .../zero_protocol/crates/zero_mesh...
Running wasm-bindgen for zero_mesh...
error: failed to read wasm-bindgen data from '.../zero_mesh.wasm'
Caused by:
    it looks like the Rust project used to create this Wasm file was linked against
    version of wasm-bindgen that uses a different bindgen format than this binary:
      rust Wasm file schema version: 0.2.130
         this binary schema version: 0.2.129
                   this binary path: /home/runner/.cargo/bin/wasm-bindgen
```

`zero_protocol/Cargo.lock` has resolved `wasm-bindgen` to `0.2.130`. However, `.github/workflows/ci.yml` in both the `test` and `deploy` jobs pins:
`cargo install -q wasm-bindgen-cli --version 0.2.129`.
Because `wasm-bindgen` schema versions between the compiled Wasm library (`0.2.130`) and the CLI binary (`0.2.129`) must match exactly, `wasm-bindgen` fails.

## 2. Proposed Changes
Update `.github/workflows/ci.yml` in `virtual_dog_walker`:
- In the `test` job, update `wasm-bindgen-cli` version from `0.2.129` to `0.2.130`.
- In the `deploy` job, update `wasm-bindgen-cli` version from `0.2.129` to `0.2.130`.

## 3. Acceptance Criteria
- **AC-012.1**: Both `wasm-bindgen-cli` install steps in `.github/workflows/ci.yml` specify `--version 0.2.130`.
- **AC-012.2**: Local test suites (`cargo test`, `python3 test/run_tests.py`, `bash scripts/build_pages.sh`) pass 100%.
- **AC-012.3**: Pushing to `origin/main` triggers the GitHub Actions CI pipeline and both `test` and `deploy` jobs complete successfully.

