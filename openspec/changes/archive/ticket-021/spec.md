# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-021"
title: "Client UI & WebRTC Integration: Distinct Leave (Selective Purge) vs Burn (Global Annihilation)"
state: "COMPLETED"
roadmap: "improved-leave-burn-lifecycle"
depends_on: ["ticket-020"]
created_at: "2026-10-18T17:05:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-021/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/improved-leave-burn-lifecycle.md`

## 1. Problem Statement & Objective
In the current web client (`zero_client/index.html` and `zero_client/src/ui.rs`), the Leave button (`#header-leave-btn`) and Burn button (`#destroy-session-button`) execute the exact same local reset function (`resetToHome()`). Furthermore, messages rendered in the DOM do not track the authoring peer ID, making it impossible to selectively purge a departed user's messages without destroying the entire chat.

This task updates the client layer to:
1. Tag each message bubble in the DOM with `data-peer-id` to identify message authorship.
2. Separate the event handlers for `#header-leave-btn` and `#destroy-session-button`:
   - `#header-leave-btn`: Broadcasts `leave_and_burn` to remote peers, wipes local DOM, and returns the local user to the home view.
   - `#destroy-session-button`: Broadcasts `global_burn` to remote peers, wipes local DOM, and returns the local user to the home view.
3. Handle incoming lifecycle frames:
   - On `leave_and_burn`: Removes all `.message-bubble[data-peer-id="..."]` for the departed peer, displays an informational system message (`[Peer left - messages burned]`), and updates peer counters.
   - On `global_burn`: Wipes the feed immediately and invokes `resetToHome()` on all receiving peers.
4. Implement `burn_peer_messages(peer_id: &str)` in `zero_client/src/ui.rs`.

---

## 2. Scope Boundaries
- **In-Scope**:
  - `zero_client/index.html`:
    - Tag message bubbles with `data-peer-id` attribute.
    - Wire `#header-leave-btn` click event to dispatch leave-and-burn signal before resetting home.
    - Wire `#destroy-session-button` click event to dispatch global-burn signal before resetting home.
    - In `ZeroWebRtcEngine` / `onMessageCb`: intercept structured lifecycle signals (`leave_and_burn`, `global_burn`).
    - Purge matching peer messages from DOM on `leave_and_burn`.
    - Trigger `resetToHome()` on `global_burn`.
  - `zero_client/src/ui.rs`:
    - Implement `burn_peer_messages(&self, peer_id: &str)` to selectively clear messages matching `peer_id`.
    - Unit tests for selective peer message deletion and session burn.
- **Out-of-Scope**:
  - Rust protocol definitions (handled in `ticket-020`).

---

## 3. Requirements & Acceptance Criteria
- [ ] **REQ-021.1 (Message Authorship Tracking)**:
  - Every rendered message bubble must include a `data-peer-id` attribute reflecting its author (`self` for local user, or the peer's unique ID for incoming messages).
- [ ] **REQ-021.2 (Distinct Leave Button Action - Selective Purge)**:
  - Clicking `#header-leave-btn` must dispatch a `leave_and_burn` frame containing the leaving user's peer ID across all active WebRTC data channels before closing connections and invoking `resetToHome()`.
- [ ] **REQ-021.3 (Distinct Burn Button Action - Global Annihilation)**:
  - Clicking `#destroy-session-button` must dispatch a `global_burn` frame across all active WebRTC data channels before closing connections and invoking `resetToHome()`.
- [ ] **REQ-021.4 (Selective Peer Message Purging on Remote Clients)**:
  - When a peer client receives a `leave_and_burn` signal for `peer_id`, all DOM message bubbles matching `[data-peer-id="<peer_id>"]` must be removed from the feed.
  - A clean system message must be logged (e.g. `Peer left - messages burned`).
  - Active peer count and peer badges must be decremented without interrupting active chat among remaining peers.
- [ ] **REQ-021.5 (Remote Global Burn Execution)**:
  - When a peer client receives a `global_burn` signal, it must immediately execute `resetToHome()`, wiping all DOM messages, closing WebRTC channels, and returning to the welcome splash screen.
- [ ] **REQ-021.6 (UI Engine Unit Tests)**:
  - `zero_client::ui` unit tests confirming `burn_peer_messages` purges only targeted peer messages while preserving other participants' messages.
