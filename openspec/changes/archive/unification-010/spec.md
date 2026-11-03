---
ticket_id: "unification-010"
title: "Remove Legacy PartyServer/Python Relays, Prune Redundant Chat/Network Code, and Rewrite Sanitized README.md"
state: "COMPLETED"
created_at: "2026-11-03T21:05:00Z"
target_repos:
  - "../virtual_dog_walker"
max_review_cycles: 3
---

# Change Proposal: unification-010 — Legacy Server/Chat Cleanup & Sanitized Architecture Documentation

## 1. Problem Statement & Audit Findings
With the four-repository zero-network architecture (`zero_protocol`, `zero_network`, `zero_chat`, and `virtual_dog_walker`) 100% unified, running over blind dead-drop WebRTC matchmaking, and fully integrated with the embedded `ZeroChatWidget`, `virtual_dog_walker` still contains residual legacy server artifacts, superseded chat/network modules, obsolete test files, and an outdated README.md that exposes local machine absolute paths:
1. **Obsolete Server Implementations**:
   - `party/server.js` (and the `party/` directory) was superseded by the standalone `zero_network` Cloudflare Worker (`DeadDropRoom`) and client-side `ZeroMeshNetworkManager`.
   - `server/host.py` and `server/ws_relay.py` (and the `server/` directory) were superseded by the P2P WebRTC architecture.
   - `requirements.txt` is no longer required because the Python test suite in `test/` exclusively utilizes standard library modules.
   - `package.json` retains an obsolete dependency on `partyserver`, and `wrangler.jsonc` contains obsolete bindings for `ParkServer` Durable Objects and migrations.
2. **Redundant Network & Chat Code**:
   - `src/net/ws_network.js` (`WebSocketNetworkManager`) is unused dead code from the old PartyServer/Python relay stack.
   - `src/ui/social_hud.js` and `src/systems/social.js` retain references to old modal room connection text and superseded chat elements.
3. **Outdated & Non-Sanitized Documentation**:
   - `virtual_dog_walker/README.md` exposes personal macOS Google Drive file paths and documents superseded PartyServer/Python architectures.
   - `virtual_dog_walker/factory.yaml` references superseded project metadata.
4. **Superseded Test Files**:
   - `test/test_partyserver.py` and `test/test_ws_microserver.py` test deleted files and should be pruned from `test/run_tests.py`. Any remaining tests referencing `party/server.js` or `ws_network.js` must be updated to inspect the canonical zero-network P2P stack.

---

## 2. Target Scope & Architecture

### 2.1 File Deletions & Configuration Updates
- **Delete Directories & Files**:
  - `party/server.js` and `party/` directory.
  - `server/host.py`, `server/ws_relay.py`, and `server/` directory.
  - `requirements.txt`.
  - `src/net/ws_network.js`.
  - `test/test_partyserver.py` and `test/test_ws_microserver.py`.
- **Update `wrangler.jsonc`**:
  - Remove `"main": "party/server.js"`, `"durable_objects"`, and `"migrations"`.
  - Configure as pure static assets: keep `"name"`, `"compatibility_date"`, and `"assets": { "directory": "./" }`.
- **Update `package.json`**:
  - Set `"name": "virtual_dog_walker"`.
  - Remove `"partyserver"` from `"dependencies"`.

### 2.2 Pruning Network & Chat Modules
- **`src/net/network.js`**: Establish `ZeroMeshNetworkManager` as the primary network export. Maintain alias/fallback only if required by existing test suites.
- **`src/ui/social_hud.js` & `src/systems/social.js`**: Prune legacy room modal/party text and dead chat inputs while keeping Online (`clifton-commons`) status badge, walker count, dog customization, emotes, barks, whistles, and overhead speech bubbles intact.

### 2.3 Test Suite Modernization (`test/`)
- Remove `test_partyserver.py` and `test_ws_microserver.py` from `test/run_tests.py`.
- Audit remaining test files (`test_leash_sync.py`, `test_npc_authority.py`, `test_social_emotes.py`, `test_chat_reliability.py`, `test_zero_mesh_integration.py`) to ensure zero assertions read deleted files or paths.

### 2.4 Documentation & Metadata Sanitization
- **`factory.yaml`**: Update `project_name` to `"virtual_dog_walker"` and `tech_stack` to modern stack ("HTML5 Canvas / Vanilla ES6 JavaScript / Rust & WebAssembly (dog_walker_core + zero_mesh) / Web Audio API / Zero-Network WebRTC P2P").
- **`README.md`**: Complete rewrite with sanitized documentation, covering polyrepo architecture (`zero_protocol`, `zero_network`, `zero_chat`, `virtual_dog_walker`), Wasm compilation, static Pages serving, local development commands, and test suites. Zero absolute user paths or personal credentials.

---

## 3. Acceptance Criteria

- **AC-010.1 (Zero Legacy Server Files)**: `party/`, `server/`, `requirements.txt`, `src/net/ws_network.js`, `test/test_partyserver.py`, and `test/test_ws_microserver.py` are completely removed.
- **AC-010.2 (Sanitized Documentation & Metadata)**: `grep -rn "mattcourtney\|mattadam" README.md factory.yaml src/ test/` returns 0 matches.
- **AC-010.3 (Configuration Integrity)**: `wrangler.jsonc` has no Durable Object bindings or migrations; `package.json` has no `partyserver` dependency.
- **AC-010.4 (Clean Build & 100% Test Pass Rate)**: `bash scripts/build_wasm.sh`, `cargo test`, and `python3 test/run_tests.py` pass 100% with zero errors or references to deleted files.

