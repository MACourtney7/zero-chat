# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-009"
title: "UI Polish: Copy Icon Button, Empty Section Elimination, Cursor & Hover Refinements"
state: "IMPLEMENTED"
roadmap: "serverless-p2p-chat"
depends_on: ["ticket-008"]
created_at: "2026-10-17T15:18:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-009/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/serverless-p2p-chat.md`

## 1. Problem Statement & Objective
Following the UI cleanup in ticket-008, further user testing revealed several minor visual artifacts and interaction rough edges:
1. An explicit text banner ("✓ Copied invite link to clipboard!...") displays beneath the invite input when copying.
2. The copy link control is a text button ("Copy Link") rather than a sleek icon button.
3. The system chat message announces "Room created! Copy the invite link above and open it in a second window/tab to connect directly.", which should be concise: "Secure room created."
4. When room controls are hidden (e.g. for a joiner or once a session is established), an empty glassmorphic capsule section remains visible underneath the top header bar.
5. The "P2P Established" badge changes background and border color on hover.
6. Hovering over the brand elements ("0", "ZeroChat", "EPHEMERAL P2P") displays a text selection I-beam cursor rather than the standard pointer/arrow cursor.

The objective of this task is to address these 6 UI polish items, ensuring a clean, tactile, and professional aesthetic across all interaction states.

## 2. Scope Boundaries
- **In-Scope**:
  - `zero_client/index.html`:
    - Remove `#copy-feedback` text banner and adjust JS copy handler to provide icon-based feedback.
    - Replace "Copy Link" text button with an SVG copy icon button.
    - Update room creation system message to "Secure room created."
    - Automatically hide `#invite-section` when not populated or during connected/joiner states.
  - `zero_client/style.css`:
    - Remove hover color changes from `.peer-pill`.
    - Apply `user-select: none; cursor: default;` to `.header-brand`, `.brand-icon`, `.brand-title`, and `.brand-tagline`.
    - Add `.btn-icon` styles and CSS empty/hidden guard for `.invite-panel`.
  - Test Suite:
    - Ensure all 91 unit and integration tests continue to pass without warnings.
- **Out-of-Scope**:
  - Cryptography, WebRTC signaling, network envelope protocol.

## 3. Requirements & Acceptance Criteria
- [x] **REQ-009.1**: The "Copied invite link..." text banner is completely removed.
  - *Verification*: Clicking copy produces no separate text element under the input row.
- [x] **REQ-009.2**: The copy link button uses a sleek copy icon (SVG) instead of "Copy Link" text, and provides visual confirmation (e.g. checkmark icon toggle) upon click.
  - *Verification*: Copy button renders an icon and transitions briefly on click.
- [x] **REQ-009.3**: The room creation system message displays "Secure room created."
  - *Verification*: In host mode, chat bubble text is "Secure room created."
- [x] **REQ-009.4**: The empty section capsule underneath the top-bar is completely hidden when nothing is populated.
  - *Verification*: When joiner loads, or when host/peer connects, no empty glass card renders between the header and chat container.
- [x] **REQ-009.5**: The "P2P Established" badge does not change background or border color on hover, and displays a default cursor.
  - *Verification*: Hovering over the badge produces no style change.
- [x] **REQ-009.6**: The brand header elements ("0", "ZeroChat", "EPHEMERAL P2P") do not display an I-beam cursor, rendering the standard arrow cursor instead.
  - *Verification*: Hovering over header brand text shows `cursor: default`.
- [x] **REQ-009.7**: Workspace tests pass and quality gates are maintained.
  - *Verification*: `cargo test --workspace` passes (91/91 tests).

## 4. Architectural Design & Impacted Modules
- `zero_client/index.html`: Markup and script updates.
- `zero_client/style.css`: CSS styling and hover/cursor updates.
