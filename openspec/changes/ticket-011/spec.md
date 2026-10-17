# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-011"
title: "Dynamic Layout & Room Lifecycle Controls"
state: "IMPLEMENTED"
roadmap: "serverless-p2p-chat"
depends_on: ["ticket-010"]
created_at: "2026-10-17T16:28:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-011/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/serverless-p2p-chat.md`

## 1. Problem Statement & Objective
In the current ZeroChat UI:
1. When not in an active chat, the chat text input bar and send button remain visible (though disabled) at the bottom of the screen.
2. The "Create Private Room" action is housed in a top panel above the messages container rather than occupying the primary action slot at the bottom.
3. The top header displays a "0 peers" pill and "Burn Session" button even when disconnected and idle, instead of a clean "Disconnected" status badge.
4. When in an active room, "Burn Session" remains in the top-right header rather than serving as a full-width action bar located directly beneath the top-bar.
5. The session burn popup displays an explanatory subtitle ("All ephemeral DOM elements, keys, and session memory have been completely wiped.") and shows an I-beam cursor on hover.

The objective of this task is to implement dynamic layout transitions between idle/disconnected and active chat states, relocate the room creation and destruction controls to full-width containers, and streamline the burn notice presentation.

## 2. Scope Boundaries
- **In-Scope**:
  - `zero_client/index.html`:
    - Relocate "Create Private Room" to the bottom action bar in place of the message input bar when disconnected.
    - Style "Create Private Room" button to span the full width of the container.
    - Conditionally hide the message input bar when disconnected; display it only when in an active chat (`st === "open"`).
    - Update header controls: remove "0 Peers" and "Burn Session" when disconnected and display a "Disconnected" badge instead; display peer count when connected.
    - Relocate "Burn Session" to directly underneath the top-bar, rename wording to "Burn Room", style as a full-width button, and display it only when in active chat.
    - Remove explanatory subtitle text from the session burn popup (`feed.innerHTML`).
    - Prevent I-beam cursor on hover over the burn popup card (`cursor: default`, `user-select: none`).
  - `zero_client/style.css`:
    - Layout classes for bottom action bars (`.action-bar-bottom`, `.btn-full-width`).
    - Layout styling for under-header burn bar (`.burn-room-bar`, `.btn-burn-full`).
    - Styling for the `.status-disconnected` header badge when idle.
    - Cursor and selection prevention on `.security-alert-card`.
  - Workspace test suite:
    - Ensure all unit, integration, and UI tests pass (`cargo test --workspace`).
- **Out-of-Scope**:
  - WebRTC signaling protocol modifications in `zero_network`.
  - Mutual/remote burn messaging across peers (deferred to a separate ticket).

## 3. Requirements & Acceptance Criteria
- [x] **REQ-011.1 (Dynamic Chat Input Bar)**: The chat text input bar (`#message-form`) is completely hidden from view when the user is not in an active chat, and visible only when connected.
- [x] **REQ-011.2 (Full-Width Bottom Create Room Bar)**: When not in an active chat, the "Create Private Room" button sits in place of the chat input bar at the bottom and spans the full width of the container.
- [x] **REQ-011.3 (Header Disconnected Badge)**: When not in an active chat, "0 Peers" and "Burn Session" are hidden from the header, and a "Disconnected" badge is displayed instead. When in active chat, the "Disconnected" badge is hidden and active peer status is shown.
- [x] **REQ-011.4 (Full-Width Under-Header Burn Room)**: The "Burn Room" button is positioned directly underneath the top-bar, styled to full container width, labeled "Burn Room", and only visible whilst in an active chat.
- [x] **REQ-011.5 (Streamlined Burn Notification)**: The burn notification card displays only the `🔥 [SESSION BURNED]` indicator; the subtitle text ("All ephemeral DOM elements, keys, and session memory have been completely wiped.") is removed.
- [x] **REQ-011.6 (Burn Popup Cursor Style)**: The burn notification card displays a standard arrow pointer on hover (`cursor: default`) with text selection disabled.
- [x] **REQ-011.7 (Forensic Invariants & Test Pass)**: All workspace tests pass (`cargo test --workspace`) and zero local storage invariants remain intact.

## 4. Architectural Design & Impacted Modules
- `zero_client/index.html`: DOM structure updates for bottom action slotting, under-header burn bar, and reactive state management in `initEngineListeners()` and `createRoomAsHost()`.
- `zero_client/style.css`: Styling for full-width action bars, under-header burn container, disconnected badge, and `.security-alert-card` cursor override.
