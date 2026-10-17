# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-014"
title: "UI Refinements: Header Badges Alignment, Leave Button, Green Send Icon, and Clean System Messages"
state: "IN_PROGRESS"
roadmap: "serverless-p2p-chat"
depends_on: ["ticket-013"]
created_at: "2026-10-17T20:55:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-014/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/serverless-p2p-chat.md`

## 1. Problem Statement & Objective
In the current ZeroChat UI:
1. The peer count badge displays `"1 peer"` or `"0 peers"`. The word `"peer(s)"` clutters the compact header badge; only the numeric count should be displayed.
2. The connection status badge displays a green emoji circle (`🟢 P2P Established`). It should feature a clean padlock icon instead of the green circle.
3. The connection status badge (`.peer-pill`) uses an indigo palette (`rgba(99, 102, 241, 0.12)` with purple borders), clashing with the adjacent cyan `.peer-count-badge`. Both header badges should share consistent cyan glass styling.
4. The message input field displays `"Type an encrypted message..."` placeholder text, which should be removed.
5. Users currently have to click the large under-header "Burn Chat" bar to leave/burn a session. A compact "Leave" button should be added to the right of the header status badges, wired to the same session burn and memory wipe handler.
6. The "Send" button uses text (`Send`) and a blue gradient, whereas primary actions (like "Create Private Chat") have moved to the green emerald palette. The send button should feature an icon instead of text and be styled in emerald green.
7. System notifications in the chat feed end with trailing full stops (`"Secure chat created."`, `"Peer joined chat."`, `"Peer left chat."`), which should be removed for a cleaner, modern chat appearance.

The objective of this task is to implement these targeted visual, layout, and copy refinements in `zero_client/index.html` and `zero_client/style.css`.

## 2. Scope Boundaries
- **In-Scope**:
  - `zero_client/index.html`:
    - Peer count badge: display strictly the numeric count (e.g. `1` or `0`).
    - Connection established badge: replace `🟢` with a padlock icon (`🔒` / SVG padlock).
    - Message input: remove `"Type an encrypted message..."` placeholder.
    - Header controls: add a "Leave" button element (`#header-leave-btn`) to the right of `#peer-list`, visible only in active chat, invoking the burn session handler.
    - Send button: replace text `"Send"` with an SVG send icon and update class to `.btn-send-emerald`.
    - System messages: remove trailing periods (`"Secure chat created"`, `"Peer joined chat"`, `"Joined chat"`, `"Peer left chat"`).
  - `zero_client/style.css`:
    - `.peer-pill` styling aligned to match `.peer-count-badge` (cyan glassmorphism).
    - Compact `#header-leave-btn` button styles (rose/dark-glass accent, hover glow, responsive alignment).
    - Green send button styling (`.btn-send-emerald`, hover, focus, disabled states).
    - Updated message input styling for empty placeholder.
  - Workspace test suite:
    - Ensure all workspace tests pass (`cargo test --workspace`).
- **Out-of-Scope**:
  - WebRTC signaling protocols in `zero_network`.
  - Cryptographic primitives in `crates/zero_protocol`.

## 3. Requirements & Acceptance Criteria
- [ ] **REQ-014.1 (Numeric Peer Counter Badge)**: The peer count badge must display only the numeric count (e.g. `1`), omitting any trailing `"peer"` or `"peers"` text.
- [ ] **REQ-014.2 (Padlock Icon on Established Badge)**: The green circle icon (`🟢`) on the connection established badge must be replaced with a padlock icon.
- [ ] **REQ-014.3 (Header Badge Color Consistency)**: The connection established badge (`.peer-pill`) must share the exact same background, border, and color palette as the peer count badge (`--accent-cyan` glass styling).
- [ ] **REQ-014.4 (Removal of Input Placeholder Text)**: The placeholder text `"Type an encrypted message..."` on `#message-input` must be completely removed.
- [ ] **REQ-014.5 (Header Leave Button)**: A "Leave" button must be positioned to the right of the peer count and connection status badge in the header. It must only be visible during active chat and must execute the exact same session burn / wipe sequence as "Burn Chat".
- [ ] **REQ-014.6 (Green Send Button with Icon)**: The "Send" button text must be replaced with an inline SVG send icon, and the button styled in emerald green matching `.btn-emerald`.
- [ ] **REQ-014.7 (Clean System Messages without Full Stops)**: All system messages rendered in the chat feed must omit trailing periods (`"Secure chat created"`, `"Peer joined chat"`, `"Joined chat"`, `"Peer left chat"`).
- [ ] **REQ-014.8 (Workspace Test Integrity)**: All 86 unit and integration tests across the workspace (`cargo test --workspace`) must pass without regressions.

## 4. Architectural Design & Impacted Modules
- `zero_client/index.html`:
  - Header controls markup: addition of `#header-leave-btn`.
  - Footer message form markup: send button icon integration and empty placeholder.
  - JavaScript lifecycle handlers in `initEngineListeners()`, `createRoomAsHost()`, `joinRoomFromHash()`, and `burnBtn`.
- `zero_client/style.css`:
  - Harmonized `.peer-pill` and `.peer-count-badge` rules.
  - Styling for `#header-leave-btn` and `.btn-send-emerald`.
