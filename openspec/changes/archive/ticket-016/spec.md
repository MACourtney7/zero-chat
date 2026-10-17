# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-016"
title: "UI Tweaks: Merged Peers Counter, Home Splash Reset, System Message Timestamps, Minimalist Splash Card, and Unified Chat Bubble Styling"
state: "COMPLETED"
roadmap: "serverless-p2p-chat"
depends_on: ["ticket-015"]
created_at: "2026-10-17T22:55:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-016/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/serverless-p2p-chat.md`

## 1. Problem Statement & Objective
In the current ZeroChat UI:
1. The peer count badge is placed on the right side of the header controls, duplicating the brand badge on the top left. The user wants the peer counter merged directly into the "0" icon on the left of the top bar, and the peer counter badge on the right removed.
2. When a chat is burned or left, the UI displays a centered red `SESSION BURNED` card. The user wants this banner removed; instead, clicking Burn or Leave should return the user to the initial home view with the welcome splash message displayed.
3. When creating a room, the "Secure chat created" system message and the invite link are displayed, but the welcome splash card remains visible above them. The splash message should not be visible unless the chat is completely empty in the idle home state.
4. System messages (`.system-message`) are currently centered with no timestamp. The user wants system messages to be left-aligned and to have a timestamp placed to the left of the message text.
5. The welcome splash card contains explanatory headers and prompts (`"Zero-Footprint Ephemeral Messaging"`, `"Direct browser-to-browser communication with cryptographic forward secrecy"`, and `"Click Create Private Chat below to start a chat..."`). The user wants these removed so that only the feature cards remain.
6. The feature cards in the splash message have verbose technical labels. The user wants them updated to clean, concise copy:
   - Card 1: Title `"Direct P2P Connection"`, description `"Zero intermediate relays or storage servers."`
   - Card 2: Title `"Secure Communications"`, description `"All messages are end-to-end encrypted."`
   - Card 3: Title `"Zero Trace"`, description `"Ephemeral chats that burn when closed."`
7. The user (outgoing) chat bubbles differ stylistically from peer (incoming) chat bubbles:
   - Outgoing bubble header is right-aligned (`justify-content: flex-end`) with no dot, whereas incoming bubbles have `[dot] [timestamp]` on the left.
   - Outgoing bubbles lack the prominent vertical accent line down the left side that incoming bubbles have (`border-left: 3px solid ...`).
   - Both outgoing and incoming bubbles include a bulky green verification footer (`.security-badge`: `"🔒 ChaCha20-Poly1305 Verified"`).
   - Hovering over timestamps changes the cursor to an I-beam selection cursor (`text`) instead of the regular arrow cursor.

The objective of this task is to implement all of these visual, functional, and stylistic refinements across `zero_client/index.html` and `zero_client/style.css`.

## 2. Scope Boundaries
- **In-Scope**:
  - `zero_client/index.html`:
    - Header: Merge peer count display into the left `.brand-icon` (assigning `id="peer-count"` to the brand icon) and remove the right-side `.peer-count-badge`.
    - Burn & Leave handlers: Remove the red `SESSION BURNED` card; clear messages and reset the view to home state with the welcome splash card visible.
    - Room Creation & Splash Lifecycle: Explicitly hide the welcome splash upon room creation (`createRoomAsHost`) and whenever any system messages or message bubbles exist.
    - System Messages: Left-align `.system-message` elements and prepend a formatted timestamp `<span class="system-timestamp">HH:MM</span>` to the left of the system message text in `appendSystemMessage()`.
    - Splash Card Markup & Template: Strip `.welcome-header` and `.welcome-prompt` from both static HTML and the `WELCOME_SPLASH_HTML` JavaScript constant string.
    - Splash Card Feature Copy: Update title and description text across both static HTML and `WELCOME_SPLASH_HTML` for the three cards.
    - User Chat Bubbles:
      - Left-align timestamp and add a glowing emerald dot (`.self-dot` or `.peer-dot` styled with `var(--accent-emerald)`) to the top-left of user chat bubbles, matching peer bubbles (`[dot] [timestamp]`).
      - Remove `.security-badge` (`"🔒 ChaCha20-Poly1305 Verified"`) from all message bubbles.
  - `zero_client/style.css`:
    - `.brand-icon`: numeric peer counter display cleanly centered.
    - `.system-message`: set `align-self: flex-start; text-align: left; display: flex; align-items: center; gap: 8px;`.
    - `.system-timestamp`: set monospace font, muted color, font-size `0.72rem`, `cursor: default; user-select: none;`.
    - `.welcome-splash`: compact vertical padding and gap with headers/prompts removed.
    - `.message-outgoing`:
      - Add `border-left: 3px solid var(--accent-emerald);` matching peer bubbles.
      - Update `.message-outgoing .message-header` to `justify-content: flex-start; gap: 6px;`.
    - `.message-timestamp`: add `cursor: default; user-select: none;` to suppress I-beam cursor on hover.
  - Workspace test suite:
    - Ensure all workspace tests pass cleanly (`cargo test --workspace`).
- **Out-of-Scope**:
  - WebRTC signaling protocols in `zero_network`.
  - Cryptographic packet sealing in `zero_protocol`.

## 3. Requirements & Acceptance Criteria
- [x] **REQ-016.1 (Merged Top-Bar Brand Peer Counter)**: The peer counter must be merged into the "0" icon on the left of the top bar (`.brand-icon`), initializing at `0` and updating dynamically to reflect active peer count (`1`, etc.). The separate peer count badge on the right side of `.header-controls` must be removed.
- [x] **REQ-016.2 (Session Burn & Leave Home Reset)**: The red `SESSION BURNED` banner card must be removed. Clicking the Burn Chat button (`#destroy-session-button`) or Leave button (`#header-leave-btn`) must completely clear the chat feed, reset session state, and return the user to the home view with the clean welcome splash card displayed.
- [x] **REQ-016.3 (Home-Only Splash Visibility Guard)**: The welcome splash card must be immediately hidden when a room is created (`createRoomAsHost`) and whenever any message or system notification exists in the feed. The splash card must only be visible when the chat feed is completely empty on the idle home view.
- [x] **REQ-016.4 (Left-Aligned System Messages with Timestamps)**: System messages (`.system-message`) must be left-aligned (`align-self: flex-start; text-align: left;`) and must display a timestamp to the left of the text (e.g. `12:34 Secure chat created`).
- [x] **REQ-016.5 (Clean Splash Header & Prompt Stripping)**: In both `index.html` static markup and `WELCOME_SPLASH_HTML` JavaScript template, the header title `"Zero-Footprint Ephemeral Messaging"`, subtitle `"Direct browser-to-browser communication with cryptographic forward secrecy."`, and the bottom prompt container (`.welcome-prompt`) must be completely removed.
- [x] **REQ-016.6 (Updated Minimalist Feature Card Copy)**: The three feature cards in the splash message (in both static HTML and `WELCOME_SPLASH_HTML`) must be updated to:
  - Card 1: Title `"Direct P2P Connection"`, description `"Zero intermediate relays or storage servers."`
  - Card 2: Title `"Secure Communications"`, description `"All messages are end-to-end encrypted."`
  - Card 3: Title `"Zero Trace"`, description `"Ephemeral chats that burn when closed."`
- [x] **REQ-016.7 (Unified User Bubble Header & Dot)**: User (outgoing) chat bubbles must have their timestamp left-aligned (`justify-content: flex-start;`) and preceded by an emerald dot icon on the top-left, matching the layout of peer bubbles (`[green dot] [timestamp]`).
- [x] **REQ-016.8 (Green Accent Border on User Bubble)**: User (outgoing) chat bubbles must feature a 3px emerald accent border along the left edge (`border-left: 3px solid var(--accent-emerald);`), matching the accent line style of peer bubbles.
- [x] **REQ-016.9 (Removal of Padlock Verification Badges)**: The padlock icon and verification text (`.security-badge` / `"🔒 ChaCha20-Poly1305 Verified"`) must be removed from the bottom of all chat bubbles.
- [x] **REQ-016.10 (Regular Arrow Cursor on Timestamps)**: Hovering over chat bubble timestamps (`.message-timestamp` and `.system-timestamp`) must show the standard arrow cursor (`cursor: default; user-select: none;`) instead of an I-beam text selection cursor.
- [x] **REQ-016.11 (Test Suite & Non-Regression Integrity)**: All 86 workspace unit and integration tests (`cargo test --workspace`) must pass cleanly.

## 4. Architectural Design & Impacted Modules
- `zero_client/index.html`:
  - `<header class="chat-header">`:
    - Update `<span class="brand-icon" id="peer-count">0</span>`.
    - Remove `<div id="peer-count" class="peer-count-badge">0</div>` from `.header-controls`.
  - `#welcome-splash` & `WELCOME_SPLASH_HTML`:
    - Retain only `.welcome-features` containing the three updated `.feature-card` items.
    - Remove `.welcome-header` and `.welcome-prompt`.
  - JavaScript functions:
    - `appendMessageBubble(senderName, text, isSelf, peerId)`:
      - If `isSelf`, prepend an emerald dot `<span class="peer-dot self-dot"></span>` to `header` before `time`.
      - Remove `const badge = document.createElement("div"); ... bubble.appendChild(badge);`.
    - `appendSystemMessage(text)`: prepend `<span class="system-timestamp">HH:MM</span>` and call `hideWelcomeSplash()`.
    - `createRoomAsHost()`: call `hideWelcomeSplash()`.
    - `burnBtn` click handler: remove `SESSION BURNED` template, clear `feed.innerHTML = ""`, reset `isBurned = false`, and call `showWelcomeSplash()`.
- `zero_client/style.css`:
  - `.system-message`: set `align-self: flex-start; text-align: left; display: flex; align-items: center; gap: 8px;`.
  - `.system-timestamp`: set monospace font, muted color, font-size `0.72rem`, `cursor: default; user-select: none;`.
  - `.message-outgoing`:
    - `border-left: 3px solid var(--accent-emerald);`
  - `.message-outgoing .message-header`:
    - `justify-content: flex-start;`
  - `.self-dot`:
    - `background-color: var(--accent-emerald);`
    - `box-shadow: 0 0 6px var(--accent-emerald);`
  - `.message-timestamp`:
    - `cursor: default;`
    - `user-select: none;`
  - `.welcome-splash`: compact vertical padding and gap with headers/prompts removed.
