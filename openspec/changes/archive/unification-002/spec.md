# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "unification-002"
title: "Add Random-Peer Dead-Drop Matchmaking in zero_network and Binary/State Mesh Framing in zero_protocol"
state: "COMPLETED"
created_at: "2026-11-01T16:40:00Z"
base_branch: "main"
ticket_branch: "factory/unification-002/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

Following Step 1 (`unification-001`), the foundational protocol, mesh engine, signaling worker, and chat client are partitioned into independent sibling repositories (`zero_protocol`, `zero_network`, `zero_chat`, and `virtual_dog_walker`).

Before migrating `virtual_dog_walker` onto Cloudflare Pages and `zero_mesh` in Step 3, two critical capabilities must be implemented in `zero_protocol` and `zero_network`:

1. **Invite-Free Public Dead-Drop Matchmaking (`zero_network` + `zero_protocol`)**:
   - In `zero_chat`, a joiner obtains an `InvitePayload` out-of-band via `#payload=...`.
   - In `virtual_dog_walker`, a player opening `play.zero-network` joins the `clifton-commons` mesh automatically without an invite link, while still utilizing the exact same `InvitePayload -> DropPayload` ChaCha20-Poly1305 encrypted WebRTC dead-drop handshake.
   - Connected hosts stage pre-generated single-use `InvitePayload`s on their hibernating WebSocket's `HostAttachment` (via `POST /stage/:do_id` or WebSocket message).
   - When a newcomer calls `GET /match/:do_id`, `DeadDropRoom` selects an active host socket with an unburned `staged_invite: Some(InvitePayload)` using pseudo-random / round-robin selection, atomically clears/claims that staged invite (preventing collision), and returns the `InvitePayload`.
   - If no staged invite exists (`status: "empty"`), the newcomer becomes a seed host on `GET /host/:do_id` and stages an invite. Once connected to a single peer, `MeshTopology`'s existing `PeerJoined` + `RelaySdpOffer`/`RelaySdpAnswer` snaps them into the full mesh with zero further Worker hits.
   - Additionally, `process_dead_drop` must route joiner `DropPayload`s to the specific matching host socket in multi-host rooms, rather than only inspecting the first host socket.

2. **Authenticated Application State / Binary Broadcast in `zero_mesh` & `zero_protocol`**:
   - `MeshRelaySignal` in `crates/zero_protocol/src/envelope.rs` currently only supports `BroadcastChat`.
   - Applications like `virtual_dog_walker` require high-frequency encrypted state/event payloads (player movement, dog leash physics, ball throws, barks, proximity chat invites) over `MeshTopology` using pairwise X25519 + ChaCha20-Poly1305 session keys (`seal_raw` / `open_raw`).

---

## 2. Scope Boundaries

### In Scope
- **`zero_protocol` (`crates/zero_protocol`)**:
  - Add `BroadcastState` variant to `MeshRelaySignal` with `packet_uuid`, `origin_peer_id`, `topic`, `ttl`, `sealed_payload`.
  - Add `StageInviteRequest` (`pub invite: InvitePayload`) and `MatchResponse` (`Matched { invite: InvitePayload }`, `Empty`).
  - Unit tests verifying JSON serialization/deserialization.
- **`zero_mesh` (`crates/zero_mesh`)**:
  - Add state broadcast and direct transmission APIs to `MeshTopology`: `on_state`, `broadcast_state`, `send_direct_state`.
  - Handle incoming `BroadcastState` in `handle_incoming_message`: deduplication, hop decryption (`open_raw`), callback invocation, downstream relay with `ttl - 1`.
  - Expose JS/Wasm bindings for `broadcast_state`, `send_direct_state`, and `on_state`.
  - Unit & multi-node tests verifying broadcast and direct transmission.
- **`zero_network`**:
  - Update `HostAttachment` with `#[serde(default)] pub staged_invite: Option<InvitePayload>`.
  - Implement `process_stage_invite` supporting `POST /stage/:do_id` and WebSocket message staging.
  - Implement `GET /match/:do_id` (`process_match_request`) returning `MatchResponse` and atomically consuming the staged invite.
  - Fix multi-host routing in `process_dead_drop` to inspect all host sockets and deliver to the token-matching host socket.
  - Comprehensive unit tests covering staging, matching, atomic consumption, and multi-host dead-drop routing.
- **Regression Verification**:
  - Verify that `zero_chat` tests pass without regression.

### Out of Scope
- Modifying `virtual_dog_walker` codebase (scheduled for Step 3).
- Deploying Cloudflare Workers to production.

---

## 3. Affected Components & Target Architecture

```text
zero_protocol/
  crates/zero_protocol/src/envelope.rs  -> Add MeshRelaySignal::BroadcastState, StageInviteRequest, MatchResponse
  crates/zero_protocol/src/lib.rs       -> Re-export new wire types
  crates/zero_mesh/src/mesh.rs          -> Add broadcast_state, send_direct_state, on_state, handle BroadcastState
  crates/zero_mesh/src/wasm.rs / lib.rs -> Expose JS/Wasm bindings
zero_network/
  src/durable_object.rs                 -> HostAttachment.staged_invite, process_stage_invite, process_match_request
  src/handlers.rs                       -> HTTP routes /stage/:do_id, /match/:do_id, targeted process_dead_drop
  src/lib.rs                            -> Route dispatching
zero_chat/
  Regression checks only
```

---

## 4. Verification & Acceptance Criteria

- **AC-002.1 (`zero_protocol` & `zero_mesh` State Broadcast)**:
  `cargo test` in `../zero_protocol` passes all tests including `MeshRelaySignal::BroadcastState`, `StageInviteRequest`, `MatchResponse`, `MeshTopology::broadcast_state`, and `MeshTopology::send_direct_state`.
- **AC-002.2 (`zero_network` Random-Peer Dead-Drop Matchmaking)**:
  `cargo test` in `../zero_network` passes all tests including `/stage/:do_id`, `/match/:do_id`, and multi-host targeted `DropPayload` routing.
- **AC-002.3 (`zero_chat` Regression Check)**:
  `cargo test` in `../zero_chat` continues to pass 100% with zero regressions.

---

## 5. Execution Plan & Cognitive Sizing

- **Brief 1 [M] (`zero_protocol` Wire Types & `zero_mesh` State Broadcast)**:
  - Add wire types to `zero_protocol/src/envelope.rs`.
  - Add state broadcast, direct state send, incoming handler, and wasm bindings in `zero_mesh`.
  - Add 3-node simulation tests.
- **Brief 2 [M] (`zero_network` Staging, Matchmaking & Multi-Host Dead-Drop)**:
  - Update `HostAttachment` and `process_stage_invite` (HTTP & WS).
  - Implement `process_match_request` with atomic consumption.
  - Fix multi-host socket targeting in `process_dead_drop`.
  - Add unit tests for matching, staging, and multi-host routing.
- **Brief 3 [L] (Integration & Regression Testing)**:
  - Verify multi-crate build and `cargo test` across `zero_protocol`, `zero_network`, and `zero_chat`.

