# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-029"
title: "Multi-User Chat Usability and Mesh Lifecycle Fixes"
state: "IN_PROGRESS"
created_at: "2026-10-19T21:12:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-029/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & Objective
Testing of the multi-user mesh chat functionality revealed usability friction in the home view and critical state synchronization defects across multi-participant sessions:

1. **Home View Create Chat Button & Share Link Simplification**:
   - The current flow requires clicking "Create Private Chat" and then interacting with a separate read-only input bar (`#invite-bar`) and copy button (`#copy-invite-button`).
   - The user experience should be streamlined: eliminate the `#invite-bar` and copy button view. Clicking "Create Chat" in the home view should immediately copy the share link to the user's clipboard and display a clear on-click feedback message indicating the link was copied.
2. **Peer Counter Synchronization Across All Clients**:
   - When a 3rd client joins, the peer count badge (`#peer-count`) is only incremented on the client that generated the invite link.
   - Root causes: hardcoded `"peer-01"` ID collisions prevent secondary peer identification, glare condition logic in `signal.PeerJoined` fails when both joining peers evaluate `isJoining == true`, and only the host was triggering peer introductions.
3. **Selective Message Purging on Peer Departure**:
   - When a 3rd participant leaves, their messages are not cleared from other clients' feeds.
   - Root cause: `leavingPeerId` in `onMessageCb` prioritized `incomingPeerId` over the explicit `peer_id` in the leave signal, misattributing the departing peer to the intermediate relay peer.
4. **Preserving Messages & Session Continuity When Original Pair Member Leaves**:
   - When participant 1 or 2 (the initial pair) leaves a 3-user chat, all messages in the feed are mistakenly wiped on surviving clients.
   - Root cause: due to failure of direct secondary mesh snapping, surviving clients fell to 0 connections and triggered the total session wipe branch. With direct mesh channels intact, surviving clients must remain connected, purge only the departed peer's messages, and keep each other's messages intact.
5. **Inconsistent Peer System Messages**:
   - Remote peer join notices display inconsistent text, leaking internal debug identifiers such as `"Peer (peer-svomrn) joined chat"` instead of clean user-facing notices. All peer system notices must be standardized.

---

## 2. Scope Boundaries
- **In-Scope**:
  - `zero_client/index.html`:
    - Remove `#invite-bar` (input field and copy button).
    - Simplify `#btn-create-room` in `#create-room-bar`.
    - Automatically copy invite link to clipboard upon room creation and display clear on-click feedback.
    - Remove hardcoded `"peer-01"` references and propagate real, unique peer IDs in invite payloads (`host_peer_id`) and dead-drop answers (`joiner_peer_id`).
    - Fix glare avoidance in `PeerJoined` to use strictly deterministic `myPeerId < remotePeerId`.
    - Trigger peer introductions whenever any connected peer adds a new participant.
    - Fix `leavingPeerId` extraction to prioritize `data.peer_id` from the leave payload.
    - Standardize system messages: `"Secure chat created"`, `"Joined chat"`, `"Peer joined chat"`, and `"Peer left chat"`.
  - `zero_client/style.css`:
    - Clean up `#invite-bar` styles.
    - Style the simplified Create Chat button and copied feedback states.
  - `tests/` and `zero_client/src/ui.rs`:
    - Update/add automated tests validating multi-peer counter synchronization, selective message purge upon departure of non-host and host peers, and UI presence states.
- **Out-of-Scope**:
  - Modifications to Cloudflare Worker edge or Durable Object core logic.

---

## 3. Requirements & Acceptance Criteria

- [ ] **REQ-029.1 (Simplified Home View Create Chat & Direct Clipboard Copy)**:
  - The read-only `#invite-bar` and `#copy-invite-button` are removed from the home view.
  - Clicking `#btn-create-room` initiates the room, immediately copies the generated invite URL to `navigator.clipboard`, and displays a clear on-click indication (e.g. button feedback state and system notice) that the share link was copied.
  - While waiting for a peer to join, subsequent clicks re-copy the active invite link and re-display the copied feedback.
- [ ] **REQ-029.2 (Consistent Peer System Messages)**:
  - System messages for peer arrival must strictly and consistently read `"Peer joined chat"` without leaking internal IDs like `(peer-svomrn)`.
  - Departures must consistently read `"Peer left chat"`.
  - Self-joining reads `"Joined chat"`, and room creation reads `"Secure chat created"`.
- [ ] **REQ-029.3 (Peer Counter Synchronization Across Full Mesh)**:
  - Real, unique peer IDs are used across all signaling envelopes (`host_peer_id`, `joiner_peer_id`).
  - Glare tie-breaker is strictly `myPeerId < remotePeerId`, enabling all secondary peers to establish direct DataChannels.
  - When a 3rd peer joins, every client in the room updates `#peer-count` in unison to reflect the correct number of connected peers (e.g. `2` on all 3 clients).
- [ ] **REQ-029.4 (Selective Message Purge on 3rd Peer Departure)**:
  - When a 3rd peer departs (`leave_and_burn` or channel close), `leavingPeerId` accurately resolves to the 3rd peer's ID across all clients.
  - Only the 3rd peer's message bubbles (`.message-bubble[data-peer-id="..."]`) are purged from remaining clients' feeds.
  - Surviving peers emit a single `"Peer left chat"` notice and decrement `#peer-count`.
- [ ] **REQ-029.5 (Feed Continuity on Host/Original Peer Departure)**:
  - When the 1st or 2nd user leaves a multi-user chat with surviving participants, surviving participants maintain direct DataChannels.
  - Only the departing peer's messages are purged; other surviving peers' messages and own messages are strictly preserved.
  - The chat remains active until the last peer leaves.
- [ ] **REQ-029.6 (Workspace Integrity & Test Verification)**:
  - All workspace tests pass (`cargo test --workspace`), zero clippy warnings (`cargo clippy --workspace --all-targets -- -D warnings`), and `wasm32-unknown-unknown` compiles cleanly.

---

## 4. Verification Plan

| Test ID | Description | Expected Result |
|---|---|---|
| `V-029-1` | Click `#btn-create-room` in home view | Invite link copied to clipboard, on-click feedback displayed, `#invite-bar` absent |
| `V-029-2` | 3-node simulation: Alice, Bob, Charlie join | All 3 nodes report `peer_count == 2`, system messages read `"Peer joined chat"` |
| `V-029-3` | Charlie sends message then departs | Only Charlie's messages purged from Alice and Bob; Alice and Bob keep own/each other's messages; peer count drops to 1 |
| `V-029-4` | Alice (host) departs from 3-node room | Only Alice's messages purged from Bob and Charlie; Bob and Charlie keep messages and continue chatting |
| `V-029-5` | Workspace test suite | `cargo test --workspace` passes 100% |
