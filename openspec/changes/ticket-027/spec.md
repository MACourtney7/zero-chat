# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-027"
title: "Header Add-Peer Button, Single-Invite Lifecycle & Multi-User Room UI"
state: "PLANNING"
roadmap: "multi-user-mesh-chat"
depends_on: ["ticket-026"]
created_at: "2026-10-18T21:02:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-027/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/multi-user-mesh-chat.md`

## 1. Problem Statement & Objective
In an active multi-user room, users need an intuitive, non-wasteful way to invite additional participants, and the room UI must adapt dynamically as participants join and leave:

1. **Header Add-Peer Button Placement**:
   - An Add-Peer button (`#header-add-btn` with `+` icon) must be placed in the header directly to the **left** of the `#peer-count` badge.
   - Visible only during active chat sessions; hidden during idle/disconnected states.
2. **Strictly Non-Wasteful Single-Invite Lifecycle**:
   - Only **one active invite** may be pending at any given time to avoid spawning redundant cloud signaling sessions or orphan WebRTC connections.
   - When clicked, generates one invite offer bundle, registers the dead-drop token for this room, and copies the link to the clipboard.
   - If an invite is already pending, subsequent clicks re-copy the existing valid link without spawning duplicate connections.
   - When the invited peer connects (DataChannel opens), the pending invite is consumed/cleared, allowing generation of the next invite.
3. **No Tooltip Messages**:
   - The link is copied silently to `navigator.clipboard`; no tooltip or popup bubble is rendered.
4. **Dynamic Multi-Peer UI**:
   - Peer counter `#peer-count` dynamically updates (`0`, `1`, `2`, `3`...).
   - Each peer is assigned a distinct color from `PEER_COLOR_PALETTE`.
   - When 1 of N peers departs (e.g. 3 -> 2), only that peer's messages are purged, a single `"Peer left chat"` notice is emitted, and the surviving peers remain in active chat.
   - Only when the peer count reaches `0` does the UI transition to the disconnected room state.

---

## 2. Scope Boundaries
- **In-Scope**:
  - `zero_client/index.html`:
    - Header DOM layout: insert `#header-add-btn` to the left of `#peer-count`.
    - Single active invite state tracking (`pendingInviteUrl`, `pendingInvitePeerId`).
    - Silent clipboard copying without tooltip.
    - Dynamic peer counter increment and decrement.
    - Per-peer message bubble coloring and attribution.
    - Multi-peer partial departure handling.
  - `zero_client/src/ui.rs`:
    - Update `ChatUi` test double to track dynamic peer additions, removals, and multi-peer selective purges.
- **Out-of-Scope**:
  - Backend Durable Object modifications.

---

## 3. Requirements & Acceptance Criteria
- [ ] **REQ-027.1 (Header Add-Peer Button Placement)**:
  - Button placed directly to the left of `#peer-count` badge in the header. Visible only in active chat.
- [ ] **REQ-027.2 (Single Active Invite Lifecycle & Zero Cloud Waste)**:
  - Generates exactly one pending invite bundle at a time.
  - Re-clicking while pending re-copies the existing valid link without spawning duplicate cloud sessions.
  - Clears pending state upon successful peer channel establishment.
- [ ] **REQ-027.3 (Silent Copy & No Tooltip)**:
  - Copies link directly to clipboard; strictly no tooltip element rendered.
- [ ] **REQ-027.4 (Dynamic Multi-Peer Counter & Partial Departure)**:
  - `#peer-count` displays accurate peer count (e.g. 2, 3...).
  - When 1 of N peers leaves, only their messages are purged; session remains active for surviving peers with decremented counter.
- [ ] **REQ-027.5 (Workspace Integrity & Tests)**:
  - Full workspace test suite passes with 0 failures and 0 clippy warnings.
