# Change Proposal: unification-007

```yaml
---
ticket_id: "unification-007"
title: "Fix ZeroMesh Seed Host Local NPC Population, Add Nighttime NPC Lighting Cutouts, and Finalize Pages Deployment Config"
state: "COMPLETED"
created_at: "2026-11-02T21:33:00Z"
target_repos:
  - "../virtual_dog_walker"
max_review_cycles: 3
current_review_cycle: 0
---
```

## 1. Problem Statement & Motivation
Inspecting `play.zero-network.workers.dev` in a live browser shows that ambient park NPCs are completely missing. A deep dive into `src/net/zero_mesh_network.js`, `src/main.js`, and `src/world/environment.js` reveals two concrete defects:

1. **Seed Host `initNpcSpawner()` Populates `this.activeNpcs` Without Local Callback Dispatch**:
   - In `ZeroMeshNetworkManager._connectLiveMesh()`, when the client becomes the seed host (`this.isHost = true`), it invokes `this.initNpcSpawner()`.
   - `initNpcSpawner()` generates pre-warmed NPCs into `this.activeNpcs` (an internal Map) and returns without ever invoking `this.onNpcSnapshot(...)` or `this.onNpcSpawn(...)`.
   - In `despawnNpc()`, replacements are broadcasted over WebRTC via `this.broadcast(spawnPacket)`, but `this.onNpcSpawn` and `this.onNpcDespawn` are never invoked locally.
   - In `src/main.js` (`CliftonDownGame`), `this.isNetworkAuthoritative()` returns `true` for the host, causing `updateLocalSpawner()` to return early and disable solo NPC generation. Because `ZeroMeshNetworkManager` fails to invoke `onNpcSnapshot` / `onNpcSpawn` locally, `game.npcs` remains permanently empty.

2. **Night/Dusk Darkness Mask Obscures Ambient NPCs**:
   - During `TimeOfDay.NIGHT` (and `DUSK`), `src/world/environment.js` draws an 82% darkness mask (`rgba(10, 16, 32, 0.82)`) over the canvas.
   - `src/main.js` only passes `[this.owner, this.dog, ...remotePlayers]` into `this.environment.render(...)` for radial visibility cutouts (`destination-out`).
   - Ambient NPCs in `this.npcs` are excluded from the cutout list, burying walkers and joggers under dark blue-black opacity unless standing directly beneath a streetlamp.

---

## 2. Architecture & Scope of Changes

### 2.1 Local Callback Invocations in `ZeroMeshNetworkManager` (`src/net/zero_mesh_network.js`)
- At the end of `initNpcSpawner()`, immediately invoke `this.onNpcSnapshot(snapshot, Date.now())` if defined, using `Array.from(this.activeNpcs.values())`.
- In `despawnNpc(id)`:
  - Dispatch local `this.onNpcDespawn(npc.id)` alongside `this.broadcast(despawnPacket)`.
  - In the replacement timer callback, dispatch local `this.onNpcSpawn(spawnPacket.npc || spawnPacket)` alongside `this.broadcast(spawnPacket)`.
- In `src/main.js` (`CliftonDownGame.initNetworking()`):
  - Add fallback snapshot consumption: if `this.net.activeNpcs && this.net.activeNpcs.size > 0 && this.npcs.size === 0` after callback registration, immediately trigger `this.handleNpcSnapshot(Array.from(this.net.activeNpcs.values()), Date.now())` to prevent any registration race condition.

### 2.2 Nighttime & Dusk Visibility Cutouts for NPCs (`src/main.js` & `src/world/environment.js`)
- In `src/main.js` `render()`:
  - Include all active NPCs from `this.npcs.values()` into the `players` (or visibility entities) array passed to `this.environment.render(...)`.
  - Alternatively/additionally, configure `environment.js` with radial illumination cutouts (`pRadius = 55..60`) for active NPCs so pedestrians are visually identifiable and illuminated during nighttime hours in Bristol.

### 2.3 Automated Regression Tests & Verification (`test/test_zero_mesh_integration.py`)
- Add `test_seed_host_populates_local_game_npcs_via_on_npc_snapshot` asserting that a host client immediately populates `game.npcs` via `onNpcSnapshot` and updates via `onNpcSpawn`/`onNpcDespawn`.
- Run full test suite in `virtual_dog_walker` (`python3 test/run_tests.py`) and across all workspace crates (`cargo test`).

---

## 3. Acceptance Criteria

- **AC-007.1 (Seed Host Local NPCs Visible Immediately)**: `ZeroMeshNetworkManager.initNpcSpawner()` invokes `this.onNpcSnapshot` locally, and `despawnNpc()` invokes `this.onNpcDespawn` and `this.onNpcSpawn` locally so the host sees active NPCs immediately without remote peers.
- **AC-007.2 (Nighttime NPC Visibility)**: Ambient NPCs receive radial illumination cutouts during `DUSK` and `NIGHT` in `src/main.js` and `src/world/environment.js`.
- **AC-007.3 (All Test Suites Green)**: `python3 test/run_tests.py` and `cargo test` pass 100% across all 4 repositories.

