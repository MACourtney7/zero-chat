# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-017"
title: "Splash Card Update: Centered Value Proposition, 3-Step Flow, and Refined Feature Copy"
state: "IN_PROGRESS"
roadmap: "serverless-p2p-chat"
depends_on: ["ticket-016"]
created_at: "2026-10-18T16:11:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-017/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/serverless-p2p-chat.md`

## 1. Problem Statement & Objective
The welcome splash card on the idle home view currently displays three feature cards without an overarching brand proposition or user guidance flow. The user wants to update the splash message layout and copy to:
1. Feature a top-centered primary heading: `"Zero accounts. Zero servers. Zero history."`
2. Feature a top-centered secondary step flow directly underneath: `"Create Chat -> Share Link -> Talk P2P"`
3. Display the updated 3-item feature list directly below the header:
   - **Direct P2P Connection**: `"Standalone browser-to-browser data channels with no intermediate relays or storage servers."`
   - **Secure Communications**: `"All messages are end-to-end encrypted."`
   - **Ephemeral Sessions**: `"Chats are permanently destroyed the moment they are closed."`

The objective of this task is to implement this exact formatting and copy across both the static DOM `#welcome-splash` in `zero_client/index.html`, the dynamic JavaScript template `WELCOME_SPLASH_HTML`, and associated layout styling in `zero_client/style.css`.

## 2. Scope Boundaries
- **In-Scope**:
  - `zero_client/index.html`:
    - Update static `#welcome-splash` in `<main id="messages-container">` with `.welcome-header` containing the centered title and step flow subtitle, followed by the updated 3 feature cards.
    - Synchronize the `WELCOME_SPLASH_HTML` JavaScript constant string with the identical markup structure and copy.
  - `zero_client/style.css`:
    - Ensure `.welcome-header`, `.welcome-title`, and `.welcome-subtitle` styling provides clean centering, legible font sizing, and harmonious vertical rhythm within `.welcome-splash`.
  - Workspace test suite:
    - Ensure all workspace tests pass cleanly (`cargo test --workspace`).
- **Out-of-Scope**:
  - WebRTC signaling protocols in `zero_network`.
  - Cryptographic packet sealing in `zero_protocol`.

## 3. Requirements & Acceptance Criteria
- [ ] **REQ-017.1 (Centered Splash Headline & Step Flow)**: The top of the welcome splash card must display a centered header section (`.welcome-header`):
  - Primary title: `"Zero accounts. Zero servers. Zero history."` (`.welcome-title`, centered).
  - Subtitle step flow: `"Create Chat -> Share Link -> Talk P2P"` (`.welcome-subtitle`, centered directly underneath).
- [ ] **REQ-017.2 (Feature 1 - Direct P2P Connection)**: The first feature card must display:
  - Title: `"Direct P2P Connection"`
  - Description: `"Standalone browser-to-browser data channels with no intermediate relays or storage servers."`
- [ ] **REQ-017.3 (Feature 2 - Secure Communications)**: The second feature card must display:
  - Title: `"Secure Communications"`
  - Description: `"All messages are end-to-end encrypted."`
- [ ] **REQ-017.4 (Feature 3 - Ephemeral Sessions)**: The third feature card must display:
  - Title: `"Ephemeral Sessions"`
  - Description: `"Chats are permanently destroyed the moment they are closed."`
- [ ] **REQ-017.5 (Dual-Template Consistency)**: The updated layout and copy must be identical across both the static HTML markup in `zero_client/index.html` and the `WELCOME_SPLASH_HTML` template string in `<script>` so that resets (burn/leave) render the exact same splash view.
- [ ] **REQ-017.6 (Visual Styling & Spacing)**: The header and feature list within `.welcome-splash` must have balanced padding, typography, and line-height in `zero_client/style.css`.
- [ ] **REQ-017.7 (Test Suite & Non-Regression Integrity)**: All 86 workspace unit and integration tests (`cargo test --workspace`) must pass cleanly.

## 4. Architectural Design & Impacted Modules
- `zero_client/index.html`:
  - Static `#welcome-splash`:
    ```html
    <div id="welcome-splash" class="welcome-splash">
        <div class="welcome-header">
            <h2 class="welcome-title">Zero accounts. Zero servers. Zero history.</h2>
            <p class="welcome-subtitle">Create Chat -&gt; Share Link -&gt; Talk P2P</p>
        </div>
        <div class="welcome-features">
            <!-- 3 updated cards: Direct P2P Connection, Secure Communications, Ephemeral Sessions -->
        </div>
    </div>
    ```
  - JavaScript `WELCOME_SPLASH_HTML`: mirror the above structure.
- `zero_client/style.css`:
  - Verify `.welcome-header` (`text-align: center; margin-bottom: 4px;`), `.welcome-title`, and `.welcome-subtitle` layout and contrast.
