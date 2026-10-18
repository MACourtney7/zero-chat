# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-023"
title: "Automatic Message Purge on Dirty Peer Disconnect & Window Unload"
state: "IN_PROGRESS"
roadmap: "improved-leave-burn-lifecycle"
depends_on: ["ticket-022"]
created_at: "2026-10-18T18:38:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-023/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/improved-leave-burn-lifecycle.md`

## 1. Problem Statement & Objective
In `ticket-021` and `ticket-022`, selective message purging was bound to explicit user interactions (clicking `#header-leave-btn` or dispatching `MeshRelaySignal::PeerLeftAndBurn`). However, when a user leaves abruptly—such as closing the browser window, navigating away, refreshing the tab, or experiencing an unannounced network severance (a "dirty disconnect")—peer clients currently only mark the peer as disconnected without purging their messages from local feeds.

This task extends the selective burn lifecycle to dirty disconnects:
1. **Mesh Layer Trigger**: When a peer's underlying DataChannel or connection terminates (`PeerLifecycleState::Closed`), `MeshTopology` must automatically invoke `on_burn_peer_messages(peer_id)` in addition to `on_peer_left(peer_id)`.
2. **Client UI Fallback & Attribution Purge**: When `zero_client/index.html` detects that a peer's WebRTC channel has closed (`st === "closed"` or disconnect), it must query and remove all message bubbles corresponding to that departed peer (`[data-peer-id="..."]`) and display an informational system message `[Peer disconnected - messages burned]`.
3. **Opportunistic Page Lifecycle Dissemination**: Hook `window.addEventListener("beforeunload", ...)` and `window.addEventListener("pagehide", ...)` in `zero_client/index.html` to best-effort broadcast a `leave_and_burn` frame across all active DataChannels before the browsing context terminates.
4. **Integration Validation**: Add automated integration tests in `zero_client/tests/mesh_simulation_test.rs` ensuring abrupt channel closure triggers selective message purging across surviving mesh nodes.

---

## 2. Scope Boundaries
- **In-Scope**:
  - `zero_client/src/mesh.rs`:
    - In `remove_peer` and the DataChannel state change listener (`PeerLifecycleState::Closed`), invoke `on_burn_peer_messages(peer_id)` so surviving peers automatically purge messages of the disconnected peer.
  - `zero_client/index.html`:
    - In `engine.onStateChange` (`st === "closed"`): selectively purge messages authored by the disconnected peer (`data-peer-id`), display `[Peer disconnected - messages burned]`, and update peer counters.
    - Add `beforeunload` / `pagehide` listeners to broadcast `leave_and_burn` signal across active data channels.
  - `zero_client/tests/mesh_simulation_test.rs`:
    - Add integration test `test_req_dirty_disconnect_triggers_peer_message_purge`: 3-node connected mesh (`Alice`, `Bob`, `Charlie`). Bob experiences an abrupt channel disconnect without prior signaling; verify that Alice and Charlie detect the disconnect, drop all of Bob's messages from their feeds, and continue communicating.
- **Out-of-Scope**:
  - Cloudflare worker edge signaling modifications.

---

## 3. Requirements & Acceptance Criteria
- [ ] **REQ-023.1 (Mesh Disconnect Message Burn Trigger)**:
  - When `MeshTopology` removes a peer or detects `PeerLifecycleState::Closed`, it must trigger both `on_peer_left_callbacks` and `on_burn_peer_messages_callbacks` for that `peer_id`.
- [ ] **REQ-023.2 (Client UI Abrupt Disconnect Purge)**:
  - When WebRTC state transitions to `"closed"` in `zero_client/index.html`, any messages attributed to the disconnected peer in `#messages-feed` must be removed from the DOM.
  - A clean system message `[Peer disconnected - messages burned]` must be logged to inform the remaining user.
- [ ] **REQ-023.3 (Opportunistic Unload Signaling)**:
  - In `zero_client/index.html`, `beforeunload` and `pagehide` event handlers must best-effort dispatch a `leave_and_burn` frame over open DataChannels before teardown.
- [ ] **REQ-023.4 (Automated Multi-Peer Dirty Disconnect Integration Test)**:
  - An automated integration test `test_req_dirty_disconnect_triggers_peer_message_purge` in `tests/mesh_simulation_test.rs` must verify that abruptly severing a peer's channel triggers selective message purging on surviving nodes while preserving surviving mesh communication.
- [ ] **REQ-023.5 (Workspace Regression & Linter Integrity)**:
  - Full workspace test suite (`cargo test --workspace`) passes with zero failures.
  - Zero clippy warnings across all workspace targets.
