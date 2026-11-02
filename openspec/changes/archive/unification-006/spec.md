# Change Proposal: unification-006

```yaml
---
ticket_id: "unification-006"
title: "Fix Cloudflare Worker/Pages Static Asset Serving, Retire Legacy PartyServer Worker Config, and Harden Zero-Network End-to-End Integration"
state: "COMPLETED"
created_at: "2026-11-02T21:03:00Z"
target_repos:
  - "../virtual_dog_walker"
  - "../zero_network"
max_review_cycles: 3
current_review_cycle: 0
---
```

## 1. Problem Statement & Motivation
When loading `play.zero-network.workers.dev` in a standard browser, users observe a blank dark green screen (`#2b4920` background from `src/style.css:44`) without canvas rendering or HUD elements. Root cause analysis reveals multiple critical issues spanning asset packaging, ES module imports, and backend API routing:

1. **Fatal Unbundled Browser ES Module JSON Import**:
   - `src/net/zero_mesh_network.js:7` executes `import npcManifest from '../world/npc_path_manifest.json';`.
   - In unbundled browser ES module contexts (`<script type="module" src="src/main.js">`), bare JSON imports without `with { type: 'json' }` fail evaluation immediately with `TypeError: Failed to load module script: Expected a JavaScript module script but the server responded with a MIME type of "application/json"`.
   - Because ES module imports are evaluated before `src/main.js` executes, this single statement halts the entire application bootstrap before `CliftonDownGame` can mount the canvas or HUD.

2. **Cloudflare Worker Static Asset Bloat & Missing `.assetsignore`**:
   - `virtual_dog_walker/wrangler.jsonc` points assets to `./`, which attempts to upload massive Rust build directories (`target/`), Git history, factory logs, and tests.
   - Missing `.assetsignore` risks upload failures, excessive bundle size, and improper MIME handling.

3. **Missing Static Obstacle Collider Registration in `src/main.js`**:
   - `dog_walker_core` collision simulation requires static colliders registered via `WasmWorldSim.set_colliders(...)` once `initWasmSim()` resolves.

4. **Missing Browser WebSocket Query-Param Secret Authentication & CORS Support in `zero_network`**:
   - Browser `new WebSocket(url)` cannot supply custom HTTP headers (`X-Host-Secret` or `Authorization`). `ZeroMeshNetworkManager._setupHostSocket` passes `?secret=${secretHex}` in the query string.
   - `zero_network` must authenticate hosts via query parameter `?secret=<hex>` in addition to headers, and must respond with full CORS headers (`Access-Control-Allow-Origin: *`, `Access-Control-Allow-Methods: GET, POST, OPTIONS`, `Access-Control-Allow-Headers: Content-Type, Authorization, X-Host-Secret`) and handle `OPTIONS` preflight requests on `/ice`, `/match/*`, `/stage/*`, and `/drop/*`.

---

## 2. Architecture & Scope of Changes

### 2.1 Browser Module Fixes & Static Obstacle Registration (`../virtual_dog_walker`)
- Remove bare `import npcManifest from '../world/npc_path_manifest.json'` in `src/net/zero_mesh_network.js`.
- Import `NPC_PATHS` from `../world/npc_paths.js` and derive manifest metadata dynamically or use the robust `FALLBACK_MANIFEST`.
- Audit all files in `src/` to guarantee zero bare JSON module imports.
- In `src/main.js`, invoke `WasmWorldSim.set_colliders(...)` once `initWasmSim()` succeeds so the Rust physics engine receives static colliders from `this.field`.
- Add `.assetsignore` to exclude `target/`, `crates/`, `.factory/`, `.git/`, `node_modules/`, `server/`, `test/`, `openspec/`, `scripts/`, `Cargo.toml`, `Cargo.lock`, `factory.yaml`, `requirements.txt`, `*.py`, and `*.md`.
- Update `package.json` scripts to build Wasm bundles before deploy (`bash scripts/build_wasm.sh && wrangler deploy`).

### 2.2 Backend Authentication & CORS Hardening (`../zero_network`)
- In `src/auth.rs` and `src/handlers.rs`:
  - Extend secret validation to parse `?secret=<hex>` from query strings when `X-Host-Secret` and `Authorization` headers are absent.
  - Apply standard permissive CORS headers (`Access-Control-Allow-Origin: *`, `Access-Control-Allow-Methods: GET, POST, OPTIONS`, `Access-Control-Allow-Headers: *`) across all handlers.
  - Return `204 No Content` for all `OPTIONS` preflight requests.

### 2.3 Verification & Automated Regression Suites
- Add `test_no_bare_json_imports_in_browser_src` to `test/test_zero_mesh_integration.py` in `virtual_dog_walker`.
- Add unit/integration tests in `zero_network` verifying query parameter secret auth on `/host/:do_id` and CORS headers across `/match/:do_id` and `/stage/:do_id`.
- Ensure all tests pass across all repositories (`python3 test/run_tests.py` and `cargo test`).

---

## 3. Acceptance Criteria

- **AC-006.1 (Zero Bare JSON Imports in Browser Modules)**: No `.js` files in `virtual_dog_walker/src/` contain bare `import ... from '*.json'` statements; browser module evaluation completes cleanly without MIME/syntax errors.
- **AC-006.2 (Clean Asset Packaging & Obstacle Wiring)**: `.assetsignore` exists in `virtual_dog_walker`, excluding non-production assets; `src/main.js` registers static obstacles with `dog_walker_core`.
- **AC-006.3 (WebSocket Query Auth & CORS in zero_network)**: `zero_network` accepts `?secret=<hex>` on `GET /host/:do_id` and serves standard CORS headers on all HTTP routes and `OPTIONS` preflights.
- **AC-006.4 (100% Test Pass Rate)**: `python3 test/run_tests.py` passes in `virtual_dog_walker` and `cargo test` passes across `zero_protocol`, `zero_network`, `zero_chat`, and `virtual_dog_walker`.

