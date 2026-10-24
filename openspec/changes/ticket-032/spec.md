# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-032"
title: "Canonical Pre-Assigned Peer IDs and Resilient Full-Mesh Snapping"
state: "IN_PROGRESS"
created_at: "2026-10-24T12:50:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-032/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & Root Cause Analysis

In manual browser testing of a 3-user chat session, the network topology degraded into an unroutable star topology ($B \leftrightarrow A \leftrightarrow C$):
1. Client 1 created a chat; Client 2 joined. Both reported `#peer-count = 1`.
2. Client 1 generated a second invite link (`+` button); Client 3 joined.
   - Client 1 reported `#peer-count = 2`.
   - Client 2 and Client 3 both reported `#peer-count = 1` (Client 2 and Client 3 failed to establish a direct DataChannel).
3. Client 1 (the initial host) departed.
   - Client 2 and Client 3 both saw `#peer-count` drop to `0`.
   - Client 2 and Client 3 were completely disconnected and unable to message each other.
   - On both surviving clients, `"Peer left chat"` was logged twice.

### Root Cause Analysis:
1. **Cloudflare Worker Serde Stripping**: In `zero_network/src/handlers.rs`, the HTTP `POST /drop/:do_id/:uuid` endpoint parses the request body into `DropPayload` and re-serializes it over the host WebSocket. Because `DropPayload` in `crates/zero_protocol/src/envelope.rs` lacked a `joiner_peer_id` field, Serde silently stripped `joiner_peer_id` during re-serialization.
2. **Peer ID Desynchronization**: On Client 1, `drop.joiner_peer_id` arrived as `undefined`, causing Client 1 to fall back to `invitePeerId` (`peer-abc`). Meanwhile, Client 3 used its own self-generated random `localPeerId` (`peer-333`).
3. **Unroutable In-Band Signaling**: When Client 1 introduced Client 2 and Client 3 via `PeerJoined`, Client 2 dispatched `RelaySdpOffer` targeted to `to_peer_id: "peer-abc"`. Client 3 received this packet, saw that `to_peer_id !== myPeerId` (`"peer-abc" !== "peer-333"`), and treated the packet as an unroutable bridge frame rather than an offer directed to itself, never generating an answer.
4. **Duplicate Departure Emissions**: In `onMessageCb`, `handlePeerDeparture(leavingPeerId)` was invoked directly, while `close_peer(leavingPeerId)` asynchronously triggered `onPeerStateChangeCb("closed")`, which also invoked `handlePeerDeparture(leavingPeerId)`.

---

## 2. Scope Boundaries

- **In-Scope**:
  - `zero_client/index.html`:
    - Generate `assigned_peer_id` for invitees during room/invite creation and pack into `#payload=` URL hash.
    - Adopt `assigned_peer_id` on join (`joinRoomFromHash`), setting `localPeerId` and `engine.localPeerId` before creating answers.
    - Connect answers against `invite.host_peer_id` so joiners store the host under their canonical ID.
    - Eliminate `rename_peer` and identity drift.
    - Scope `clearPendingInvite()` to match `peerId === pendingInvitePeerId`.
    - Fix departure order: call `handlePeerDeparture` before `close_peer` and ensure `departedPeers` suppresses duplicate notices.
  - `crates/zero_protocol/src/envelope.rs`:
    - Add `#[serde(default)] pub host_peer_id: Option<String>` and `pub assigned_peer_id: Option<String>` to `InvitePayload`.
    - Add `#[serde(default)] pub joiner_peer_id: Option<String>` to `DropPayload`.
  - `zero_network/src/handlers.rs` & tests:
    - Ensure `DropPayload` propagates `joiner_peer_id` to host WebSocket.
  - `tests/` & `zero_client/tests/`:
    - Update regression test suites to validate pre-assigned canonical peer IDs and full-mesh survival on host departure.
- **Out-of-Scope**:
  - Replacing the Cloudflare Worker architecture.
  - Introducing third-party TURN servers.

---

## 3. Requirements & Acceptance Criteria

- [ ] **REQ-032.1 (Pre-Assigned Canonical Peer IDs in URL Fragment)**:
  - When Client 1 creates a room (`createRoomAsHost`) or generates an invite (`handleHeaderAddPeerClick`), it generates `assigned_peer_id = "peer-" + randomHex(6)`.
  - The offer is created under `assigned_peer_id` in `engine.peers`.
  - The URL hash `#payload=` contains both `host_peer_id: localPeerId` and `assigned_peer_id: assigned_peer_id`.
  - In `joinRoomFromHash`, the joiner sets its own `localPeerId = invite.assigned_peer_id` and targets the host via `invite.host_peer_id`.
  - No `rename_peer` calls or identity remapping races occur.
- [ ] **REQ-032.2 (Protocol Schema Support in zero_protocol & zero_network)**:
  - `InvitePayload` includes `host_peer_id: Option<String>` and `assigned_peer_id: Option<String>`.
  - `DropPayload` includes `joiner_peer_id: Option<String>`.
  - Serde deserialization and serialization in Cloudflare Worker preserves `joiner_peer_id` across the dead-drop pipe.
- [ ] **REQ-032.3 (Deterministic Full-Mesh In-Band Snapping)**:
  - When a peer channel transitions to `"open"` on any node, introductions (`PeerJoined`) are dispatched using the canonical IDs.
  - The tie-breaker `myPeerId < remotePeerId` evaluates symmetrically on Client 2 and Client 3.
  - `RelaySdpOffer` routes to `to_peer_id === myPeerId`, generating a `RelaySdpAnswer` that establishes direct $B \leftrightarrow C$ DataChannel.
  - In a 3-user session (A, B, C), all 3 clients report `#peer-count = 2`.
- [ ] **REQ-032.4 (Scoped Pending Invite Cleanup)**:
  - `clearPendingInvite()` only closes `pendingInviteWs` when `peerId === pendingInvitePeerId`, preventing secondary peer channel transitions from terminating an active invite socket.
- [ ] **REQ-032.5 (Clean Single Departure & Host Departure Survivability)**:
  - When Client 1 leaves, pairwise $B \leftrightarrow C$ DataChannel remains open and active.
  - Both Client 2 and Client 3 update `#peer-count` from `2` to `1`.
  - Messages sent by Client 2 are received and displayed by Client 3 (and vice versa).
  - Only one `"Peer left chat"` notice is displayed across surviving clients.
- [ ] **REQ-032.6 (Workspace Integrity & Verification)**:
  - `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `wasm32-unknown-unknown` builds pass cleanly.
  - Headless browser test suite and simulation tests pass 100%.

---

## 4. Verification Plan

| Test ID | Description | Expected Result |
|---|---|---|
| `V-032-1` | Schema serialization in `crates/zero_protocol` | `DropPayload` with `joiner_peer_id` roundtrips without field loss |
| `V-032-2` | Dead-drop handler test in `zero_network` | Worker preserves `joiner_peer_id` when piping answer to host WebSocket |
| `V-032-3` | Multi-node simulation test in Rust | 3-node join with canonical IDs forms full mesh; host departs; survivors maintain direct link |
| `V-032-4` | Headless browser test harness | Client 2 and Client 3 connect via pre-assigned IDs; both show 2 peers; host leaves, count drops to 1, messages exchange |
| `V-032-5` | Departure notice deduplication | Single `"Peer left chat"` emitted on peer exit |
| `V-032-6` | Real browser 3-tab verification | Manual confirmation in 3 browser tabs matching user scenario |

---

## 5. Proposed Brief Decomposition (for `implement-task`)

| Brief | Sizing | Files | Description |
|---|---|---|---|
| `brief-impl-032-protocol` | LOW | `crates/zero_protocol/src/envelope.rs`, `zero_network/src/handlers.rs`, tests | Add schema fields and test dead-drop serialization roundtrip |
| `brief-impl-032-browser` | MEDIUM | `zero_client/index.html`, `zero_client/tests/browser/*` | Pre-assigned peer IDs in URL, joiner ID adoption, scoped invite cleanup, departure dedup |
| `brief-validation` | LOW | reports only | Run full workspace fmt, clippy, tests, wasm, and browser test suite |
| `reviewer` | HIGH | reviews/ | Code review cycle (max 3 cycles) |
