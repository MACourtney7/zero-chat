---
ticket_id: "unification-013"
title: "Phase 1: Complete dog_walker_core Wasm sim wiring and polish embedded ZeroChatWidget"
state: "COMPLETED"
created_at: "2026-11-06T21:52:00Z"
target_repos:
  - "../virtual_dog_walker"
  - "../zero_chat"
max_review_cycles: 3
---

# Change Proposal: unification-013 — Phase 1: Wasm Simulation Wiring & ZeroChatWidget Polish

## 1. Audit Findings (verified against current `main`)

### 1.1 Wasm simulation is only partially authoritative
`src/engine/wasm_sim_bridge.js` already exports `stepEarSprings`, `stepLeashConstraint`, `resolveWorldCollision`, `sampleNpcState` (all with JS fallback) and `getWasmSim()`. Call-site audit:

| Wasm function | Bridge helper | Current gameplay call site | Status |
| :--- | :--- | :--- | :--- |
| `step_leash` | `stepLeashConstraint` | `companion_dog.js:471` | WIRED |
| `resolve_collision` | `resolveWorldCollision` | `owner.js:88`, `companion_dog.js:516` still call `field.resolveCircleCollision(...)` (JS) | NOT WIRED |
| `step_ears` | `stepEarSprings` | `companion_dog.js` ~L525-540 inline JS spring (`springK`, `springDamp`) | NOT WIRED |
| `step_ball` | (none; `toy.js` imports `getWasmSim` but `update()` runs inline gravity/bounce) | `toy.js:75-120` | NOT WIRED |
| `sample_npc` | `sampleNpcState` | `npc.js` imports it but `ParkNPC.setClock()` (L150+) runs inline polyline math | NOT WIRED |

### 1.2 Design risks that must be resolved in the implementation
1. **`step_ball` is stateful and single-ball**: `WasmWorldSim` holds one `self.ball` and `throw_ball(start, target, power)` uses a *target/power* API while JS `TennisBall.throw(from, dir, speed, launchZ)` uses direction/speed/launchZ and the game has per-player balls (remote players' balls are also simulated). Delegating naively would make all balls share one state. Required: add a **stateless** Rust entry point (e.g. `step_ball_state(x,y,z,vx,vy,vz,dt) -> [..]`, parameterised with gravity 720 / restitution 0.62 / friction 0.94 matching `toy.js`) or per-ball handles, plus native Rust tests. Gameplay behaviour must be unchanged (fetch mechanics tests must still pass).
2. **Parity**: JS fallback must remain bit-for-bit identical in behaviour when Wasm is unavailable (headless tests, load failure).
3. **Collision data**: `resolve_collision` relies on `set_colliders` having been called (`main.js:205/241`). Entities must fall back to `field.resolveCircleCollision` when Wasm colliders are not yet loaded.
4. **NPC determinism**: P2P NPC coordination relies on all peers computing the same position from `spawnTime`/`now`. `sample_npc` (f64) must produce results equal to the JS path within 1e-6 px, and `opacity`/`finished` semantics (0.5 s fade after finish, `done` flag) must be preserved.

### 1.3 ZeroChatWidget polish gaps
- `src/style.css` (1131 lines) still holds legacy chat rules superseded by `zero_chat_widget.css`: `.proximity-message*` (L701–~800), `#proximity-chat-input-container`, `.proximity-emoji-bar`, `.proximity-emoji-btn*` (L805–~845) and related. Existing tests couple to part of this (`test_proximity_chat.py:1008-1010` asserts `.proximity-message-sender`/`.proximity-message-text` in style.css; `test_ticket_023_refinements.py` asserts `#proximity-chat-input { width: 100% }` and absence of emoji bar in JS). These tests must be re-pointed to `zero_chat_widget.css` / widget classes, not weakened.
- `ZeroChatWidget.setPeers()` (`zero_chat_widget.js:402`) only updates the text `"N nearby"`; no participant names/colored dots in the header.
- `#chat-toggle-btn` (built in `proximity_chat.js:151`) has no unread indicator when the drawer is collapsed.

## 2. Scope

### 2.1 Track A — Wasm simulation wiring (`virtual_dog_walker`, `crates/dog_walker_core`)
- A1 [HIGH] **Rust**: add stateless ball step API to `crates/dog_walker_core` (+ native `cargo test` cases for gravity, bounce restitution, rolling friction, obstacle deflection parity); rebuild bundle via `scripts/build_wasm.sh`. Files: `crates/dog_walker_core/src/lib.rs` (+ `ball_physics.rs`).
- A2 [MEDIUM] **Bridge + entities**: add `stepBallState` helper (with exact JS fallback) to `wasm_sim_bridge.js`; switch `TennisBall.update` to it; switch `Owner.update` and `CompanionDog.update` obstacle resolution to `resolveWorldCollision` (fallback to `field.resolveCircleCollision` when colliders absent); switch ear springs to `stepEarSprings`. Files: `wasm_sim_bridge.js`, `toy.js`, `owner.js`, `companion_dog.js`.
- A3 [MEDIUM] **NPC**: make `ParkNPC.setClock` delegate to `sampleNpcState` while preserving fade/done/walkPhase/angle semantics. Files: `npc.js`.
- A4 [LOW] **Tests**: extend `test/test_wasm_core_sim.py` with source-contract assertions that each of the five entity call sites uses the bridge, and JS-vs-Wasm parity checks where runnable headless (JSC/node harness if available).

### 2.2 Track B — ZeroChatWidget polish (`zero_chat` widget is canonical → synced to `virtual_dog_walker/src/ui/`)
- B1 [MEDIUM] **Widget**: in `zero_chat/zero_client/widget/zero_chat_widget.{js,css}` extend `setPeers(peerIds, count, peerMeta?)` to render a participant strip in the header (colored `.peer-dot` + truncated name pills using `PEER_COLOR_PALETTE`, max 4 + "+N"), keep existing `#peer-count` text and IDs; add `setUnreadCount(n)`/`clearUnread()` hooks. Update `zero_client/tests/test_zero_chat_widget.js` (+ `.py` runner). Must not alter `zero_client/index.html` inline script contract (`run_test.py` regex) or the 24 browser tests.
- B2 [MEDIUM] **Host**: in `virtual_dog_walker/src/ui/proximity_chat.js`, pass peer metadata (name/color from `remotePlayers`) into `setPeers`; add unread badge element to `#chat-toggle-btn` (increment on incoming remote message while drawer is collapsed, clear on open; not for own/system messages). Styles for the badge live in `zero_chat_widget.css` or `style.css` under a `.chat-toggle-badge` class.
- B3 [LOW] **CSS cleanup**: delete the superseded `.proximity-message*`, `#proximity-chat-input-container`, `.proximity-emoji-*` rules from `src/style.css`; re-point the coupled tests to widget classes/CSS. Retain `#chat-toggle-btn`, `#proximity-chat-panel`, `#proximity-chat-messages`, `#proximity-chat-input` rules still in use.

## 3. Acceptance Criteria
- **AC-013.1**: `TennisBall.update`, `Owner.update`, `CompanionDog.update` (collision + ears) and `ParkNPC.setClock` each call a `wasm_sim_bridge.js` helper; no inline duplicate physics remains except inside the bridge's JS fallback.
- **AC-013.2**: New stateless ball API exists in `dog_walker_core` with ≥4 new native tests; `cargo test` green; rebuilt `src/wasm_core/*` committed/served via `build_wasm.sh`.
- **AC-013.3**: With Wasm unavailable (headless), all existing gameplay tests (`test_ball_physics`, `test_fetching_mechanics`, `test_npcs`, `test_npc_client`, `test_dog_behavior`, `test_leash_sync`) still pass unchanged in behaviour.
- **AC-013.4**: NPC parity: for sampled `(t)` over several manifest paths, Wasm and JS positions differ < 1e-6 px; fade/finish semantics preserved.
- **AC-013.5**: `src/style.css` no longer contains `.proximity-message`, `.proximity-emoji-*`, or `#proximity-chat-input-container`; coupled tests updated to the widget CSS and still assert the same behaviours (no test deleted without equivalent replacement).
- **AC-013.6**: Header shows participant dots/name pills for 2+ walkers (max 4 + "+N"), consistent colors with message bubbles; `#peer-count` text retained.
- **AC-013.7**: `#chat-toggle-btn` shows an unread badge (count, capped "9+") for remote messages while collapsed; cleared on open; never counts own/system messages; covered by widget/host tests.
- **AC-013.8**: `zero_chat`: `cargo test`, `bash zero_client/tests/browser/run_test.sh` (24/24), `test_zero_chat_widget.py`, `deploy_dry_run.sh` green; `virtual_dog_walker`: `cargo test`, `bash scripts/build_pages.sh`, `python3 test/run_tests.py` green; GitHub Actions CI green on both repos after push.

## 4. Proposed Brief Decomposition (parallel, disjoint file sets)
| Brief | Size | Repo | Files |
| :--- | :--- | :--- | :--- |
| `brief-rust-ball-api` | HIGH | virtual_dog_walker | `crates/dog_walker_core/src/*` |
| `brief-entity-wasm-wiring` (depends on rust-ball-api for ball only; collision/ears/NPC can start immediately → split ball into stage 2) | MEDIUM | virtual_dog_walker | `wasm_sim_bridge.js`, `owner.js`, `companion_dog.js`, `npc.js` (then `toy.js` in stage 2) |
| `brief-widget-peers-unread` | MEDIUM | zero_chat | `widget/zero_chat_widget.{js,css}`, widget tests |
| `brief-host-chat-integration` (after widget brief) | MEDIUM | virtual_dog_walker | `src/ui/proximity_chat.js`, synced widget copy |
| `brief-css-cleanup` | LOW | virtual_dog_walker | `src/style.css`, `test/test_proximity_chat.py`, `test/test_ticket_023_refinements.py` |
Stage 1 (parallel): rust-ball-api, entity-wasm-wiring (collision/ears/NPC), widget-peers-unread, css-cleanup. Stage 2: toy.js ball wiring, host-chat-integration. Then validation (all 4 repos) and review.

## 5. Risks / Notes
- Rebuilding Wasm needs `wasm-bindgen-cli 0.2.130` locally (the local CLI currently warns about a schema mismatch for `zero_chat`'s `deploy_dry_run.sh`; it falls back to copying raw wasm — unrelated to this ticket).
- Multi-repo change: `zero_chat` push must precede `virtual_dog_walker` CI so the build script's widget sync picks up the new widget.
- Gameplay feel must not change; if any parity test fails the Wasm path must be fixed, not the JS tolerance loosened.

