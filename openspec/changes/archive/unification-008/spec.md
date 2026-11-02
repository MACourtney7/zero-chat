# Change Proposal: unification-008

```yaml
---
ticket_id: "unification-008"
title: "Wire Live WebRTC Offer/Answer for Proximity Chat Mesh and Complete WasmWorldSim Entity Delegation"
state: "COMPLETED"
created_at: "2026-11-02T21:47:00Z"
target_repos:
  - "../virtual_dog_walker"
max_review_cycles: 3
current_review_cycle: 0
---
```

## 1. Problem Statement & Motivation
Inspecting the runtime code in `virtual_dog_walker` following `unification-007` reveals two remaining gaps in live WebRTC communication and physics engine integration:

1. **Incomplete Proximity Mesh WebRTC Handshake & Message Wiring (`src/net/zero_mesh_network.js`)**:
   - `syncProximityCluster()` notifies remote peers when within 160px using a placeholder `{ initiator: this.peerId, timestamp: Date.now() }` rather than creating a real `WasmWebRtcEngine` SDP offer.
   - `_handleProximityDrop()` is a stub comment (`// Proximity channel confirmed`), so the WebRTC answer is never applied to the pending engine and peers are never registered in `this.proximityMesh`.
   - `this.proximityMesh.set_on_message(...)` is never wired in `_connectLiveMesh()`, meaning messages received over proximity data channels are dropped instead of being routed to `this.onProximityChat`.
   - There is no fallback delivery over `this.gameMesh` while proximity WebRTC channels are still in ICE gathering/negotiation, risking dropped messages when players first meet.

2. **Uncalibrated WasmWorldSim Dimensions & Missing Entity Delegation (`src/engine/wasm_sim_bridge.js`, `src/entities/companion_dog.js`, `src/ui/social_hud.js`)**:
   - `src/engine/wasm_sim_bridge.js` hardcodes `WasmWorldSim(2000.0, 2000.0)` instead of the actual Clifton Downs map dimensions (3200 x 2400).
   - `CompanionDog.update()` has access to `stepLeashConstraint` from `wasm_sim_bridge.js`, but still relies entirely on JS math instead of delegating spring-damper leash physics to the Rust/Wasm engine.
   - `SocialHUD` checks `isCommons` against `!this.net.roomId || this.net.roomId === 'the-commons' || this.net.roomId === 'clifton-field'`, excluding `clifton-commons` and showing `🔒 Meadow (clifton-commons)` instead of `Online`.

---

## 2. Architecture & Scope of Changes

### 2.1 Live Dual-Mesh Proximity WebRTC Handshake (`src/net/zero_mesh_network.js`)
- Wire `this.proximityMesh.set_on_message(...)` in `_connectLiveMesh()`:
  - Normalizes sender ID (strips `prox-` prefix).
  - Retrieves peer name from `this.remotePeers`.
  - Dispatches to `this.onProximityChat({ senderId, name, text })`.
- Real In-Band Offer/Answer Exchange over `gameMesh.send_direct_state`:
  - `pendingProximityEngines`: Map tracking `remotePeerId -> WasmWebRtcEngine`.
  - `syncProximityCluster()`: When `remoteId` is within 160px and `this.peerId < remoteId`, create `engine = new WasmWebRtcEngine()`, generate `sdpOffer`, and send `proximity_invite` via `this.gameMesh.send_direct_state`.
  - `_handleProximityInvite()`: Creates `engine`, generates answer via `engine.accept_offer_and_create_answer(offer)`, registers peer in `proximityMesh`, and returns `proximity_drop` with `sdpAnswer`.
  - `_handleProximityDrop()`: Calls `engine.apply_answer(sdpAnswer)`, registers peer in `proximityMesh`, and removes engine from pending map.
  - In `sendProximityChat()`: If `proximityMesh.peer_count() === 0` while peers are nearby, provide fallback delivery over `gameMesh.send_direct_state` with `PacketType.PROXIMITY_CHAT`.

### 2.2 WasmWorldSim Calibration & Entity Delegation (`src/engine/wasm_sim_bridge.js`, `src/entities/companion_dog.js`, `src/ui/social_hud.js`)
- In `src/engine/wasm_sim_bridge.js`:
  - Calibrate default world dimensions to `worldWidth = 3200.0, worldHeight = 2400.0`.
  - Add `stepEarSprings(leftEar, rightEar, dogVx, dogVy, dogAngle, dt)` calling `wasmSimInstance.step_ears(...)`.
- In `src/entities/companion_dog.js`:
  - In `CompanionDog.update()`, invoke `stepLeashConstraint(this, this.owner, 65, 4.0, 5.0, dt)` when leashed and not fetching, smoothly cushioning leash tension prior to the 105px hard boundary clamp.
- In `src/ui/social_hud.js`:
  - Add `|| this.net.roomId === 'clifton-commons'` to `isCommons` so the HUD badge displays green `Online` for public sessions.

### 2.3 Automated Regression Tests & Verification (`test/`)
- In `test/test_zero_mesh_integration.py`:
  - Add `test_live_proximity_mesh_sdp_handshake_and_chat_routing`.
  - Add `test_social_hud_recognizes_clifton_commons_as_public_online_badge`.
- In `test/test_wasm_core_sim.py`:
  - Verify `initWasmSim` initializes with `3200.0, 2400.0`.
  - Verify `stepLeashConstraint` integration in `CompanionDog`.
- Ensure 100% test pass rate across all 4 repositories.

---

## 3. Acceptance Criteria

- **AC-008.1 (Live Dual-Mesh Proximity WebRTC Handshake)**: `ZeroMeshNetworkManager` performs SDP offer/answer exchange over `gameMesh.send_direct_state`, registers peers in `proximityMesh`, wires `proximityMesh.set_on_message` to `onProximityChat`, and includes fallback delivery over `gameMesh` while negotiating.
- **AC-008.2 (WasmWorldSim 3200x2400 & Entity Wiring)**: `initWasmSim` initializes with 3200x2400 world dimensions and `CompanionDog.update()` invokes `stepLeashConstraint`.
- **AC-008.3 (SocialHUD Public Commons Badge)**: `src/ui/social_hud.js` recognizes `clifton-commons` as the public commons and displays `Online`.
- **AC-008.4 (100% Test Suite Green)**: `python3 test/run_tests.py` and `cargo test` pass across all 4 repositories.

