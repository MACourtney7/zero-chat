# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-015"
title: "UI Tweaks: Icon-Only Controls, Domain URL Header & Tab, Favicon, and Minimalist Header Badges"
state: "IN_PROGRESS"
roadmap: "serverless-p2p-chat"
depends_on: ["ticket-014"]
created_at: "2026-10-17T21:37:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-015/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/serverless-p2p-chat.md`

## 1. Problem Statement & Objective
In the current ZeroChat UI:
1. The status connection badge displays descriptive text (`Disconnected`, `Awaiting..`). The user wants to remove text labels entirely so that only the connection circle indicator dot is displayed.
2. The peer count badge is hidden when disconnected rather than displaying `0`. It should be visible continuously, starting at `0`.
3. The background, border, and color of the `0` peers badge and status indicator badge should consistently match the default cyan/blue glass styling (`--accent-cyan`).
4. The welcome splash card currently re-appears when a peer leaves, even if the feed already contains chat messages or notifications. It should strictly display only when the chat is completely empty at the initial "Home" view.
5. The primary room creation button contains the text `"Create Private Chat"`. This should be replaced with a clean icon.
6. The connection established badge in `#peer-list` displays `"🔒 P2P Established"`. The text `"P2P Established"` should be removed so only the padlock icon remains.
7. The header `#header-leave-btn` displays `"Leave"` text. This should be replaced with an exit/logout icon.
8. The under-header `#destroy-session-button` displays `"Burn Chat"` text. This should be replaced with a flame/fire icon.
9. The top-bar brand title displays `"ZeroChat"`. This should be replaced with the domain URL (`zero-chat.pages.dev`).
10. The browser tab `<title>` displays `"ZeroChat — Serverless Ephemeral P2P Chat"`. This should be replaced with the domain URL (`zero-chat.pages.dev`).
11. The application does not define a dedicated favicon matching the rounded "0" brand icon from the header. An SVG favicon matching this design should be configured.
12. Buttons and interactive badges currently display browser native hover tooltips via `title` attributes (e.g. hovering over the send button). All hover tooltip text should be removed.

The objective of this task is to implement these visual, iconographic, and brand refinements across `zero_client/index.html` and `zero_client/style.css`.

## 2. Scope Boundaries
- **In-Scope**:
  - `zero_client/index.html`:
    - Brand title and `<title>` replacement with domain URL (`zero-chat.pages.dev` / dynamic fallback).
    - SVG favicon linked in `<head>` matching the "0" brand icon.
    - Status badge: remove `#status-badge-text`, rendering only the indicator circle dot (no hover tooltip).
    - Peer count badge: keep visible in idle state, initialized to `0`.
    - Welcome splash lifecycle: show only when `feed` is completely empty; do not re-show if feed contains messages or notifications.
    - Create Private Chat button: replace text with an SVG plus/chat icon (no hover tooltip).
    - P2P Established badge: remove text, leaving solely the padlock icon (no hover tooltip).
    - Leave button: replace `"Leave"` text with an exit/logout SVG icon (no hover tooltip).
    - Burn Chat button: replace `"Burn Chat"` text with a fire/flame SVG icon (no hover tooltip).
    - Send button & Copy button: remove `title` hover tooltips.
  - `zero_client/style.css`:
    - Status pill and peer count badge color consistency (`--accent-cyan` glass styling).
    - Icon-only button sizing, alignment, and hover glow for create room, leave, and burn chat buttons.
    - Minimalist status badge circle container padding.
  - Workspace test suite:
    - Ensure all workspace tests pass (`cargo test --workspace`).
- **Out-of-Scope**:
  - WebRTC signaling protocols in `zero_network`.
  - Cryptographic packet sealing in `zero_protocol`.

## 3. Requirements & Acceptance Criteria
- [ ] **REQ-015.1 (Icon-Only Status Badge)**: The connection status badge must display only the status indicator circle dot without any text label (`"Disconnected"` and `"Awaiting.."` removed).
- [ ] **REQ-015.2 (Persistent Peer Count Badge)**: The peer count badge must remain visible when disconnected/idle, displaying `0` initially.
- [ ] **REQ-015.3 (Cyan Glass Badge Color Consistency)**: The `0` peers badge and status indicator badge must share the default cyan/blue glass styling (`background: rgba(56, 189, 248, 0.1); border: 1px solid rgba(56, 189, 248, 0.25); color: var(--accent-cyan);`).
- [ ] **REQ-015.4 (Home-Only Welcome Splash)**: The welcome splash message must only display when the chat container is completely empty at the initial "Home" view. If the feed contains chat messages or system notifications when a peer disconnects, the splash must not re-appear over the messages.
- [ ] **REQ-015.5 (Icon-Only Create Private Chat Button)**: The text `"Create Private Chat"` on `#btn-create-room` must be replaced with a centered icon with appropriate tooltip and accessibility labels.
- [ ] **REQ-015.6 (Icon-Only Established Padlock Badge)**: The connection established badge must remove `"P2P Established"` text, displaying solely the padlock icon.
- [ ] **REQ-015.7 (Icon-Only Leave Button)**: The text `"Leave"` on `#header-leave-btn` must be replaced with an exit/door icon.
- [ ] **REQ-015.8 (Icon-Only Burn Chat Button)**: The text `"Burn Chat"` on `#destroy-session-button` must be replaced with a fire/flame icon.
- [ ] **REQ-015.9 (Domain URL Branding in Header & Tab)**: The top-bar brand title and `<title>` element must both display the domain URL (`zero-chat.pages.dev`).
- [ ] **REQ-015.10 (Brand SVG Favicon)**: A favicon matching the header "0" brand icon (rounded square with gradient glass and cyan "0") must be embedded via `<link rel="icon" type="image/svg+xml">`.
- [ ] **REQ-015.11 (Removal of Hover Tooltip Text)**: All native browser hover tooltip text (`title="..."` attributes) must be removed across buttons and badges (e.g. `#send-button`, `#copy-invite-button`, `#destroy-session-button`, `#header-leave-btn`, `#btn-create-room`, and badges) so that no tooltips appear on hover.
- [ ] **REQ-015.12 (Test & Non-Regression Integrity)**: All 86 workspace unit and integration tests (`cargo test --workspace`) must pass cleanly.

## 4. Architectural Design & Impacted Modules
- `zero_client/index.html`:
  - `<head>`: `<title>zero-chat.pages.dev</title>` and `<link rel="icon">` with data URI SVG.
  - Header brand: `<h1 class="brand-title">zero-chat.pages.dev</h1>`.
  - Header controls: `#status-badge` dot only, `#peer-count` initialized to `0` without `.hidden`.
  - Buttons: SVG icons for `#btn-create-room`, `#header-leave-btn`, and `#destroy-session-button`.
  - JavaScript: Splash display conditional guard checking `feed.querySelectorAll(".message-bubble, .system-message").length === 0`.
- `zero_client/style.css`:
  - Consistent cyan glass variables on `#status-badge` and `#peer-count`.
  - Icon button centering and padding adjustments.
