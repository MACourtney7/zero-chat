# Change Proposal: unification-005

```yaml
---
ticket_id: "unification-005"
title: "Execute dog_walker_core Rust/Wasm Simulation Port and Fix Live ZeroMeshNetworkManager WebRTC & SYNC_STATE Schema Bugs"
state: "IN_PROGRESS"
created_at: "2026-11-01T18:20:00Z"
target_repos:
  - "../virtual_dog_walker"
max_review_cycles: 3
current_review_cycle: 0
---
```

## 1. Problem Statement & Motivation
While the preliminary scaffolding of `dog_walker_core` and the Python mock tests in `test_zero_mesh_integration.py` pass, a thorough inspection of the live browser networking layer in `src/net/zero_mesh_network.js` against `src/main.js` (`CliftonDownGame`) reveals 4 critical issues preventing real-world multiplayer play on Cloudflare Pages:

1. **Incompatible `remotePeers` & `SYNC_STATE` Schema**:
   - `CliftonDownGame.update()` expects each entry in `this.net.remotePeers` to contain `{ ownerInterpolator, dogInterpolator, meta }`.
   - `ZeroMeshNetworkManager` instead stored `{ id, name, jacketColor, coatName, interpolator }` and parsed flat coordinates, whereas `Protocol.createSyncState` transmits nested `{ owner: { x, y, vx, vy, angle, isMoving, isLeashed }, dog: { x, y, vx, vy, angle, state } }`.
   - `this.onPeerJoin` was invoked as `(senderId, name, peerObj)` instead of `(senderId, meta)` where `meta` is `{ name, jacketColor, coatName, ... }`.
   - `Protocol.createWelcome` was called missing `this.localMeta` as the 2nd argument.
2. **Missing `startSyncLoop()` and `stop()` Methods**:
   - `CliftonDownGame.initNetworking()` calls `this.net.startSyncLoop(...)`, throwing `TypeError: this.net.startSyncLoop is not a function`.
3. **Dummy Hardcoded SDP & Missing `WasmCryptoSession.seal_drop_sdp`**:
   - `_connectLiveMesh()` called a non-existent method `create_drop_payload_json` with a hardcoded `"v=0..."` placeholder instead of real `WasmWebRtcEngine.accept_offer_and_create_answer` and `WasmCryptoSession.seal_drop_sdp`.
4. **Empty `_setupHostSocket()` Stubs**:
   - Seed hosts never authenticated via `?secret=${secretHex}`, never staged an `InvitePayload` to `DeadDropRoom` (`POST /stage/:do_id` or WebSocket), and had empty `onmessage` handlers that never decrypted incoming `DropPayload` answers.

Additionally, we will refine `crates/dog_walker_core` to include floppy ear spring-damper physics (`step_ears`), off-leash roaming clamping bounds (105px on-leash, 230px off-leash), and ensure dual Wasm build artifacts are tracked and packaged cleanly.

---

## 2. Architecture & Scope of Changes

### 2.1 Live WebRTC & Handshake Fixes in `src/net/zero_mesh_network.js`
- Implement `ensureRemotePeer(peerId, meta = {})`:
  - Instantiates `ownerInterpolator: new EntityInterpolator()` and `dogInterpolator: new EntityInterpolator()`.
  - Maintains `meta: { name, jacketColor, coatName, hairStyle, hairColor, pantsColor, skinTone, collarColor, personality }`.
- Fix packet handling in `handlePacket`:
  - `PacketType.HELLO`: Call `this.ensureRemotePeer(senderId, meta)` and `this.onPeerJoin(senderId, meta)`. Reply with `Protocol.createWelcome(this.peerId, this.localMeta, peersList)`.
  - `PacketType.WELCOME`: Register host and peers with `ensureRemotePeer` and trigger `this.onPeerJoin(p.id, meta)`.
  - `PacketType.SYNC_STATE`: Decode both nested `packet.owner` / `packet.dog` and flat properties, setting target on `ownerInterpolator` and `dogInterpolator`.
- Implement `startSyncLoop(getLocalStateFn)` running at 16 Hz and `stop()`.
- Real Wasm Dead-Drop Handshake:
  - Joiner: `engine.accept_offer_and_create_answer(invite.sdp_offer)` $\rightarrow$ `cryptoSession.seal_drop_sdp(...)` $\rightarrow$ `POST /drop/:roomId/:invite_uuid`.
  - Host: Connects WebSocket to `/host/:roomId?secret=${secretHex}`, creates offer via `engine.create_offer()`, signs `token = compute_hmac_hex`, stages `InvitePayload`, decrypts joiner drop in `onmessage` via `cryptoSession.open_drop_sdp`, applies answer, and stages fresh invite.

### 2.2 Ear Springs & Leash Physics in `crates/dog_walker_core`
- Add `step_ears` spring-damper routine to `dog_physics.rs` (stiffness $180.0$, damping $14.0$).
- Extend `step_leash` to support 105px on-leash clamp and 230px off-leash roam clamp (exempting `FETCHING` and `LOST`).

---

## 3. Acceptance Criteria

- **AC-005.1 (Live ZeroMesh Network Compatibility)**:
  - `ZeroMeshNetworkManager` implements `startSyncLoop`, `stop`, `ensureRemotePeer` with `{ ownerInterpolator, dogInterpolator, meta }`.
  - Accurately parses nested `Protocol.createSyncState` packets.
  - Invokes `onPeerJoin(senderId, meta)` and `Protocol.createWelcome(this.peerId, this.localMeta, peers)`.
  - Uses real `WasmWebRtcEngine` and `WasmCryptoSession` methods (`seal_drop_sdp`, `open_drop_sdp`, `compute_hmac_hex`).
- **AC-005.2 (dog_walker_core Enhancements)**:
  - `crates/dog_walker_core` passes $\ge 15$ native Rust tests, adding ear spring physics.
- **AC-005.3 (100% Green Verification Across All Repositories)**:
  - All test suites in `virtual_dog_walker`, `zero_protocol`, `zero_network`, and `zero_chat` pass.

---

## 4. Verification Plan

1. **Rust Tests**: `cargo test --manifest-path ../virtual_dog_walker/crates/dog_walker_core/Cargo.toml`
2. **Build Scripts**: `cd ../virtual_dog_walker && bash scripts/build_wasm.sh && bash scripts/build_pages.sh`
3. **Python Suite**: `cd ../virtual_dog_walker && python3 test/run_tests.py`
4. **Sibling Repos**: `cargo test` on `zero_protocol`, `zero_network`, and `zero_chat`.

