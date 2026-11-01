# Change Proposal: unification-004

```yaml
---
ticket_id: "unification-004"
title: "Stage 2 Migration: Extract Core Spatial Physics, Leash Spring-Damper, Ball Ballistics, and Deterministic NPC Interpolation into Rust/Wasm (dog_walker_core)"
state: "IN_PROGRESS"
created_at: "2026-11-01T18:07:00Z"
target_repos:
  - "../virtual_dog_walker"
max_review_cycles: 3
current_review_cycle: 0
---
```

## 1. Problem Statement & Motivation
In Step 3 (`unification-003`), `virtual_dog_walker` migrated its networking and signaling layer onto `zero_protocol` and `zero_mesh` WebAssembly primitives, enabling invite-free dead-drop matchmaking on `zero_network`, P2P game state replication, and dual-mesh proximity chat with hysteresis.

In Step 4 (`unification-004` — Stage 2), we execute the Strangler-Fig simulation port by migrating the performance-critical, deterministic core simulation logic out of JavaScript and into zero-allocation, memory-safe Rust compiled to WebAssembly (`crates/dog_walker_core`).

The existing HTML5 Canvas 2D renderer and Web Audio synthesis remain as a thin presentation layer that steps the Rust `WasmWorldSim` on each frame, falling back cleanly to pure JS math in headless test runners.

---

## 2. Architecture & Scope of Changes

### 2.1 New Rust Crate: `crates/dog_walker_core`
Scaffolded inside `virtual_dog_walker`:
- `Cargo.toml` (workspace root and crate level)
- `src/lib.rs`: `#[wasm_bindgen]` export of `WasmWorldSim` and zero-overhead snapshot transfer structures.
- `src/math.rs`: 2D/3D vector routines, shortest-arc angle interpolation (`lerp_angle`), and deterministic `mulberry32` PRNG.
- `src/collision.rs`: World boundary clamping (margin = 36, inner wall = 56) and static circle collider resolution (`resolve_entity_collisions`).
- `src/dog_physics.rs`: Spring-damper leash constraint (`step_leash_constraint`: length 110.0–180.0, spring stiffness $k$, velocity damping, obstacle avoidance, and steering).
- `src/ball_physics.rs`: 3D projectile trajectory (`BallSim`: gravity $-580.0\text{ px/s}^2$, ground bounce damping $0.55$, rolling friction $0.92$, and obstacle collision deflection).
- `src/npc_sim.rs`: Deterministic polyline path sampling (`sample_npc_state`: walker $22\text{ px/s}$, jogger $48\text{ px/s}$, `to_seconds` timestamp normalization, waypoint corner turning smoothing within 28px, and entry/exit opacity fade).

### 2.2 Dual WebAssembly Build Pipeline
- Update `scripts/build_wasm.sh` and `scripts/build_pages.sh`:
  - Build `zero_mesh` from `../zero_protocol/crates/zero_mesh` into `src/wasm/`.
  - Build `dog_walker_core` from `./crates/dog_walker_core` into `src/wasm_core/` via `wasm-bindgen --target web`.
  - Package both Wasm bundles into `dist/` for Cloudflare Pages.

### 2.3 JavaScript Bridge & Non-Regression Entity Integration
- `src/engine/wasm_sim_bridge.js`:
  - Dynamically loads `./wasm_core/dog_walker_core.js` in browser environments.
  - Exposes synchronous helper methods (`stepBallPhysics`, `stepLeashConstraint`, `resolveWorldCollision`, `computeNpcState`).
  - Automatically falls back to existing pure JS simulation math when Wasm is not yet initialized (ensuring headless Python/Node unit tests remain 100% green without modification).
- Wire bridge into:
  - `src/main.js` (`CliftonDownGame`)
  - `src/entities/toy.js` (`TennisBall.update`)
  - `src/entities/companion_dog.js` (`CompanionDog.update`)
  - `src/entities/npc.js` (`ParkNPC.setClock`)

---

## 3. Acceptance Criteria

- **AC-004.1 (dog_walker_core Rust Crate & Native Tests)**:
  - `crates/dog_walker_core` compiles cleanly with zero warnings (`cargo test`).
  - Passes at least 15 native Rust unit tests covering collision resolution, 3D ball ballistics, spring-damper leash physics, and deterministic NPC polyline interpolation.
- **AC-004.2 (Dual Wasm Build Pipeline)**:
  - `bash scripts/build_wasm.sh` and `bash scripts/build_pages.sh` compile both `zero_mesh` (into `src/wasm/`) and `dog_walker_core` (into `src/wasm_core/`) with `wasm-bindgen --target web`.
  - `dist/` contains all static assets, Wasm binaries, and JS wrappers.
- **AC-004.3 (Zero Regression Bridge Integration)**:
  - `src/engine/wasm_sim_bridge.js` delegates simulation ticks to `WasmWorldSim` when initialized while maintaining full backward-compatible fallback for JS entities.
  - `python3 test/run_tests.py` in `virtual_dog_walker` passes all existing test suites plus `test/test_wasm_core_sim.py`.
- **AC-004.4 (Multi-Repository Non-Regression)**:
  - `cargo test --manifest-path ../zero_protocol/Cargo.toml` (109 passed).
  - `cargo test --manifest-path ../zero_network/Cargo.toml` (17 passed).
  - `cargo test --manifest-path ../zero_chat/Cargo.toml` (36 passed).

---

## 4. Verification Plan

1. **Rust Unit Tests**: `cargo test --manifest-path ../virtual_dog_walker/Cargo.toml`
2. **Build Scripts**:
   - `cd ../virtual_dog_walker && bash scripts/build_wasm.sh`
   - `cd ../virtual_dog_walker && bash scripts/build_pages.sh`
3. **Python Integration Suite**:
   - `cd ../virtual_dog_walker && python3 test/test_wasm_core_sim.py`
   - `cd ../virtual_dog_walker && python3 test/run_tests.py`
4. **Multi-Repo Regression**:
   - `cargo test --manifest-path ../zero_protocol/Cargo.toml`
   - `cargo test --manifest-path ../zero_network/Cargo.toml`
   - `cargo test --manifest-path ../zero_chat/Cargo.toml`

