# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "unification-003"
title: "Migrate virtual_dog_walker to zero_mesh P2P State Replication, Random Dead-Drop Matchmaking, Dual-Mesh E2EE Proximity Chat, and Cloudflare Pages"
state: "IN_PROGRESS"
created_at: "2026-11-01T17:48:00Z"
base_branch: "main"
ticket_branch: "factory/unification-003/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

Following Steps 1 (`unification-001`) and 2 (`unification-002`):
- `zero_protocol` and `zero_mesh` expose wire types (`BroadcastState`, `StageInviteRequest`, `MatchResponse`), state streaming APIs (`broadcast_state`, `send_direct_state`, `on_state`), `WasmCryptoSession`, and `WasmMeshTopology`.
- `zero_network` exposes endpoints for invite staging (`POST /stage/:do_id` and WebSocket message staging), random-peer dead-drop matchmaking (`GET /match/:do_id`), and token-targeted routing in multi-host rooms (`POST /drop/:do_id/:invite_uuid`).

In Step 3 (`unification-003` — Stage 1 of the `virtual_dog_walker` migration), the existing JavaScript simulation and canvas rendering engine remains intact while migrating `virtual_dog_walker`'s networking, matchmaking, NPC authority, and proximity chat onto `zero_protocol` / `zero_mesh` and configuring a static Wasm build pipeline for Cloudflare Pages.

---

## 2. Core Architectural Design

### 2.1 Invite-Free Public Dead-Drop Matchmaking (`clifton-commons`)
- **ICE / TURN Discovery**: Client fetches `GET /ice` from `zero_network` (`window.WORKER_HTTP_ORIGIN || "https://network.zero-network.workers.dev"`) and caches ICE servers for 15 minutes.
- **Match Request (`GET /match/:do_id`, default `clifton-commons`)**:
  - **Matched (`MatchResponse::Matched { invite }`)**:
    - Adopts `invite.assigned_peer_id` (if present), verifies HMAC token.
    - Derives pairwise X25519 shared secret with `invite.host_public_key` via `WasmCryptoSession`.
    - Generates WebRTC SDP answer, seals it into `sealed_sdp_answer`, and submits to `POST /drop/:do_id/:invite_uuid`.
    - Upon DataChannel opening, `WasmMeshTopology` automatically exchanges `PeerJoined` + `RelaySdpOffer` / `RelaySdpAnswer` over the bridge to snap direct P2P connections to all other walkers in the park with zero further Worker hits.
    - Also connects as a host socket to `GET /host/:do_id` and stages a fresh `InvitePayload` to contribute to matchmaking for future joiners.
  - **Empty (`MatchResponse::Empty`)**:
    - Acts as seed host: generates a 32-byte `host_secret`, connects WebSocket to `GET /host/:do_id`, creates a pre-staged `InvitePayload`, and stages it via `StageInviteRequest`.
    - Upon receiving an incoming `DropPayload` from a joiner, decrypts via `open_drop_sdp`, completes connection, and replenishes its staged invite.

### 2.2 Game Mesh State Replication & Deterministic P2P NPC Authority
- **Encrypted Game Mesh (`topic: "game_packet"`)**:
  - All game packets (`HELLO`, `WELCOME`, `SYNC_STATE`, `BALL_THROWN`, `BALL_SYNC`, `BARK`, `PET`, `WHISTLE`, `EMOTE`, `CHAT_MESSAGE`, `NPC_SPAWN`, `NPC_DESPAWN`, `NPC_SNAPSHOT`) are serialized to UTF-8 JSON bytes and broadcasted via `gameMesh.broadcast_state("game_packet", bytes, 3)` (sealed per-peer with ChaCha20-Poly1305 AEAD).
- **Deterministic P2P NPC Coordinator & Reseed**:
  - The connected peer with the lexicographically lowest `peerId` (or the seed peer when alone) coordinates ambient park NPCs: initializes 5 pre-warmed NPCs, sends `NPC_SNAPSHOT` to joining peers, and broadcasts `NPC_SPAWN` / `NPC_DESPAWN`.
  - When all peers depart, the room empties; the next incoming seed peer re-seeds a fresh pool of 5 pre-warmed NPCs.

### 2.3 Dual-Mesh Ephemeral Proximity Chat
- **Strict Separation**:
  - `gameMesh` (`WasmMeshTopology`): Park-wide coordinate & entity synchronization.
  - `proximityChatMesh` (`WasmMeshTopology`): Dedicated instance isolated strictly to walkers within the conversation circle.
- **Hysteresis Thresholds**:
  - Join Radius: `160px` (`PROXIMITY_JOIN_RADIUS = 160`).
  - Leave Radius: `200px` (`PROXIMITY_LEAVE_RADIUS = 200`), with 5-minute message retention (`RETENTION_DURATION_MS = 300_000`).
- **In-Band Proximity Handshake**:
  - When two walkers enter within `160px`, the deterministic initiator (`localPeerId < remotePeerId`) creates an offer on `proximityChatMesh` and sends a `proximity_invite` payload directly via `gameMesh.send_direct_state(remotePeerId, "proximity_invite", bytes)`.
  - Recipient accepts, seals SDP answer, and returns it via `gameMesh.send_direct_state(initiatorPeerId, "proximity_drop", bytes)`.
  - Proximity chat messages use `proximityChatMesh.broadcast_chat(text)` (E2EE `BroadcastChat` frames). Walkers outside the circle never receive chat frames.
  - When a walker exceeds `200px`, their `proximityChatMesh` connection is closed, while `gameMesh` remains connected.

---

## 3. Scope Boundaries & Affected Repositories

### In Scope
- **`zero_protocol` (`crates/zero_mesh`)**:
  - Verify and export required `wasm_bindgen` functions and helpers for browser ES module consumption.
- **`virtual_dog_walker`**:
  - `scripts/build_wasm.sh` and `scripts/build_pages.sh` to compile `zero_mesh` to `src/wasm/` and package `dist/` for Cloudflare Pages.
  - `package.json` scripts: `build:wasm`, `build`, `test`.
  - `src/net/zero_mesh_network.js`: `ZeroMeshNetworkManager` implementing matchmaking, Game Mesh broadcast, NPC coordinator, and dual-mesh proximity chat with in-memory test fallback.
  - `src/net/network.js`: Integrate `ZeroMeshNetworkManager` as the default P2P/Pages adapter, preserving `WebSocketNetworkManager` (`?server=`) and `PeerNetworkManager` (`?p2p=1`).
  - `src/ui/proximity_chat.js`: Support hysteresis radii (`joinRadius: 160`, `leaveRadius: 200`) while maintaining default `radius = 200` backward compatibility for existing tests.
  - `src/main.js`: Wire `onClusterChanged` and proximity chat messaging through `ZeroMeshNetworkManager`.
  - `test/test_zero_mesh_integration.py` and `test/run_tests.py`: Unit and integration test suite.

### Out of Scope
- Rust rewrite of virtual dog walker simulation (scheduled for Stage 2).

---

## 4. Verification & Acceptance Criteria

- **AC-003.1 (Wasm & Cloudflare Pages Build Pipeline)**:
  `scripts/build_wasm.sh` and `scripts/build_pages.sh` build the `zero_mesh` Wasm bundle into `src/wasm/` and assemble `dist/` cleanly.
- **AC-003.2 (Public Dead-Drop Matchmaking & P2P Game Mesh)**:
  `ZeroMeshNetworkManager` connects to `zero_network`, replicates game state over `WasmMeshTopology::broadcast_state`, and deterministically coordinates/reseeds ambient NPCs across peers.
- **AC-003.3 (Dual-Mesh Proximity Chat with Hysteresis)**:
  Proximity chat forms at 160px and disconnects at 200px in `CliftonDownGame`, bootstrapping a separate `WasmMeshTopology` in-band over `gameMesh.send_direct_state` and encrypting chat strictly among cluster peers.
- **AC-003.4 (100% Test Suite Green Across All 4 Repos)**:
  `python3 test/run_tests.py` in `../virtual_dog_walker` (including `test_zero_mesh_integration.py`) and `cargo test` in `../zero_protocol`, `../zero_network`, and `../zero_chat` pass with zero regressions.

---

## 5. Execution Plan & Cognitive Sizing

- **Brief 1 [M] (`zero_mesh` Wasm Bindings & Build Pipeline)**:
  - Inspect and update `crates/zero_mesh` Wasm exports if needed.
  - Author `scripts/build_wasm.sh` and `scripts/build_pages.sh` in `virtual_dog_walker`.
  - Update `package.json`.
- **Brief 2 [H] (`ZeroMeshNetworkManager` & Deterministic NPC Coordination)**:
  - Implement `src/net/zero_mesh_network.js` (dead-drop client, game mesh state broadcast, deterministic NPC coordinator).
  - Update `src/net/network.js` with fallback preservation.
- **Brief 3 [M] (Dual-Mesh Proximity Chat Hysteresis & Wiring)**:
  - Update `src/ui/proximity_chat.js` with 160px/200px hysteresis logic.
  - Wire proximity cluster changes in `src/main.js`.
- **Brief 4 [M] (Integration Tests & Multi-Repo Verification)**:
  - Author `test/test_zero_mesh_integration.py` covering matchmaking, game state replication, NPC reseed, and dual-mesh proximity chat.
  - Register in `test/run_tests.py`.
  - Run full test suites across all 4 repos.

