# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-018"
title: "UI Tweaks: Adaptive Top-Bar Header Badges, Transient Ticking Status Messages, and Splash Box Styling"
state: "COMPLETED"
roadmap: "serverless-p2p-chat"
depends_on: ["ticket-017"]
created_at: "2026-10-18T16:24:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-018/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/serverless-p2p-chat.md`

## 1. Problem Statement & Objective
In the current ZeroChat UI:
1. **Top-Bar Brand & Header Controls**:
   - Left side: restore static "0" brand logo (`.brand-icon`).
   - Right side: establish fixed alignment with an adaptive badge layout:
     - From left to right: (1) Peer count, (2) Connection status, (3) Exit button.
     - **Disconnected state**: Show peer count (`0`) on the left, and a double-width red-dot ellipse badge (~60px wide, matching the width of two 26px badges + gap) on the right.
     - **Connected state**: The double-width badge splits into two distinct badges: the connection status badge (middle) and the exit button (right).
     - This gives the impression of the idle red capsule splitting into two active controls when a chat connects, while keeping the overall bounding width and alignment of the header fixed.
2. **Elimination of Blue Banner & Transient Status Messages**:
   - The blue connecting status banner (`#joiner-banner.join-banner`) is visually obtrusive and should be removed completely.
   - During connection establishment, show a transient in-feed status message: `"Establishing secure connection"` with an animated ticking ellipsis (`.`, `..`, `...`).
   - This transient status message has **no timestamp**.
   - The moment the connection opens (or fails), the transient status message is **replaced** by the permanent, timestamped system message (`"Joined chat"`, `"Peer joined chat"`, or `"Failed to establish connection"`).
   - Creating a chat immediately emits `"Secure chat created"` with timestamp.
3. **Splash Card Refinements**:
   - Update Card 1 description to: `"Private data channels with no intermediate relays or storage servers."`
   - Remove on-hover border glow from `.feature-card:hover` in `zero_client/style.css`.

## 2. Header Badges Layout Specification

### Disconnected / Idle View:
```text
┌─────────────────────────────────────────────────────────────┐
│ [0] zero-chat.pages.dev                   [ 0 ] [  ● red  ] │
│     SECURE P2P CHAT                    peer-count  double-width
│                                                    capsule (~60px)
└─────────────────────────────────────────────────────────────┘
```

### Active Chat View:
```text
┌─────────────────────────────────────────────────────────────┐
│ [0] zero-chat.pages.dev               [ 1 ] [ 🔒 ] [ 🚪 ]   │
│     SECURE P2P CHAT                 peer-count  status  exit button
│                                     (Badge 1)  (Badge 2) (Badge 3)
└─────────────────────────────────────────────────────────────┘
```
*(Combined width of Badge 2 + gap + Badge 3 = 26px + 6px + 26px = 58px, matching the disconnected capsule width!)*

## 3. Scope Boundaries
- **In-Scope**:
  - `zero_client/index.html`:
    - Top bar left: restore `<span class="brand-icon">0</span>` as static brand logo.
    - Top bar right:
      - Restore `<div id="peer-count" class="peer-count-badge">0</div>`.
      - Add `<div id="idle-status-capsule" class="status-capsule-disconnected"><span class="status-indicator-dot red-dot"></span></div>` visible when disconnected.
      - Keep `#status-badge` / `#peer-list` (connection badge) and `#header-leave-btn` (exit button) unhidden only when connected.
      - Toggle between idle capsule and active split pair upon connection/disconnect.
    - Remove `#joiner-banner`.
    - Implement transient status helper (`showTransientStatus("Establishing secure connection")` / `clearTransientStatus()`) that renders without timestamp and with ticking ellipsis.
    - When permanent system message is appended (`Joined chat`, `Peer joined chat`, `Failed to establish connection`), automatically replace/clear any active transient status message.
    - Splash card Card 1 description update: `"Private data channels with no intermediate relays or storage servers."` (both static DOM and `WELCOME_SPLASH_HTML`).
  - `zero_client/style.css`:
    - Add `.status-capsule-disconnected` with red glass styling (~58-60px width, 26px height, pill border-radius, centered red dot).
    - Remove `.join-banner` rules.
    - Remove `.feature-card:hover` border glow rule.
    - Add `.transient-status` and `@keyframes tickDots` styling for the animated 1-2-3 ticking dots.
  - Workspace test suite:
    - Ensure all workspace tests pass cleanly (`cargo test --workspace`).
- **Out-of-Scope**:
  - WebRTC signaling protocols or cryptography.

## 4. Requirements & Acceptance Criteria
- [x] **REQ-018.1 (Restore Top-Bar Brand Logo & Peer Counter)**:
  - Top-left header displays static brand logo `<span class="brand-icon">0</span>`.
  - Top-right `.header-controls` displays standalone peer counter badge `<div id="peer-count" class="peer-count-badge">0</div>` as Badge 1 on the left, initializing at `0` and updating dynamically (`1`, etc.).
- [x] **REQ-018.2 (Adaptive Split Header Badges & Fixed Alignment)**:
  - When disconnected / idle, `.header-controls` displays:
    - Left: `#peer-count` (`0`).
    - Right: `#idle-status-capsule`, a double-width red-dot capsule (~58-60px wide, 26px high) with red glass styling and a centered red indicator dot.
  - When connected in active chat, the idle capsule is hidden and splits into:
    - Middle: `#peer-list` / `#status-badge` (connection status badge).
    - Right: `#header-leave-btn` (26px red exit button).
  - Total bounding width remains visually constant between states.
- [x] **REQ-018.3 (Remove Blue Connecting Status Banner)**:
  - `#joiner-banner` and `.join-banner` rules must be completely removed from the DOM and stylesheet.
- [x] **REQ-018.4 (Transient Ticking Status Message & Replacement)**:
  - Connection negotiation must display a transient status message: `"Establishing secure connection"` with an animated ticking ellipsis (`.`, `..`, `...`) and NO timestamp.
  - When connection completes (`"Joined chat"` / `"Peer joined chat"`) or fails (`"Failed to establish connection"`), the transient status element must be removed and replaced by the permanent timestamped system message.
  - Chat creation directly logs `"Secure chat created"` with timestamp.
- [x] **REQ-018.5 (Splash Message Copy Refinement)**:
  - Card 1 description must be updated to `"Private data channels with no intermediate relays or storage servers."` across both static HTML and `WELCOME_SPLASH_HTML`.
- [x] **REQ-018.6 (Remove Feature Card On-Hover Glow)**:
  - Remove `.feature-card:hover` border color change in `zero_client/style.css` so the bullet boxes maintain an invariant border on hover.
- [x] **REQ-018.7 (Clean Splash Visibility Guard)**:
  - The welcome splash card must continue to hide immediately when any transient status or system message is rendered.
- [x] **REQ-018.8 (Test Suite & Non-Regression Integrity)**:
  - All 86 workspace unit and integration tests (`cargo test --workspace`) must pass cleanly.
