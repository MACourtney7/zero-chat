# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-020"
title: "Protocol Envelopes & Mesh Routing for Selective Message Purging and Global Burn"
state: "IN_PROGRESS"
roadmap: "improved-leave-burn-lifecycle"
depends_on: []
created_at: "2026-10-18T17:05:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-020/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/improved-leave-burn-lifecycle.md`

## 1. Problem Statement & Objective
In the current zero_chat architecture, the protocol layer supports basic signaling relay and chat broadcasts, but lacks explicit signals for selective peer message destruction and global session annihilation. When a peer leaves or an active chat is burned, no signal is transmitted across the P2P mesh to inform remote peers, causing messages to persist on remote clients.

This task extends `crates/zero_protocol` and `zero_client::mesh` with:
1. `MeshRelaySignal::PeerLeftAndBurn { peer_id: String }` to inform peers to tear down the connection and purge all messages originating from the departed peer.
2. `MeshRelaySignal::GlobalBurnSession { initiator_peer_id: String }` to inform all peers in the mesh to destroy their entire local chat session simultaneously.
3. Broadcast and callback hooks in `MeshTopologyManager` for firing and intercepting these lifecycle signals.

---

## 2. Scope Boundaries
- **In-Scope**:
  - `crates/zero_protocol/src/envelope.rs`:
    - Add `PeerLeftAndBurn { peer_id: String }` variant to `MeshRelaySignal`.
    - Add `GlobalBurnSession { initiator_peer_id: String }` variant to `MeshRelaySignal`.
    - Implement serialization, deserialization, and TTL forwarding test vectors.
  - `zero_client/src/mesh.rs`:
    - Implement `broadcast_leave_and_burn(&self) -> Result<(), MeshError>` on `MeshTopologyManager`.
    - Implement `broadcast_global_burn(&self) -> Result<(), MeshError>` on `MeshTopologyManager`.
    - Implement `on_burn_peer_messages` and `on_global_burn` callback registries.
    - Wire incoming frame handling to trigger appropriate callbacks and prune internal routing tables.
- **Out-of-Scope**:
  - DOM manipulation or browser HTML elements (handled in `ticket-021`).

---

## 3. Requirements & Acceptance Criteria
- [ ] **REQ-020.1 (Protocol Signal Definitions)**:
  - `MeshRelaySignal` must contain `PeerLeftAndBurn { peer_id: String }` and `GlobalBurnSession { initiator_peer_id: String }`.
  - Both signals must serialize and deserialize cleanly to/from JSON and binary formats via `zero_protocol::envelope`.
- [ ] **REQ-020.2 (Mesh Broadcast Routines)**:
  - `MeshTopologyManager` must expose `broadcast_leave_and_burn(&self)` which broadcasts the signal to all active peers before disconnecting.
  - `MeshTopologyManager` must expose `broadcast_global_burn(&self)` which floods the annihilation signal to all active peers before wiping local state.
- [ ] **REQ-020.3 (Mesh Signal Ingestion & Callbacks)**:
  - When `PeerLeftAndBurn` is received from a peer, the manager removes the peer from the topology and executes registered `on_burn_peer_messages` callbacks with `peer_id`.
  - When `GlobalBurnSession` is received, the manager cleans all routing tables, severs connections, and executes registered `on_global_burn` callbacks.
- [ ] **REQ-020.4 (Unit Test Coverage)**:
  - Protocol roundtrip tests for the new signals in `zero_protocol`.
  - Unit tests in `zero_client::mesh` verifying callback invocation and mesh state teardown upon signal reception.
