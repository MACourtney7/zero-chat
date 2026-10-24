# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-033"
title: "Simplified Icon-Only Room Creation and Temporary System Messages"
state: "COMPLETED"
created_at: "2026-10-24T14:02:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-033/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

The current room creation button and system message lifecycle contain visual clutter and inconsistent feedback:
1. **Bottom Button Text & Multi-State Transitions**: The bottom button (`#btn-create-room`) currently includes text labels (`"Create Chat"`, `"Link copied to clipboard!"`, `"Share link copied · Waiting for peer..."`) across multiple timer-driven states. The user desires a simplified, icon-only button flow with exactly 2 visual stages.
2. **System Message Transience**: All system messages currently display timestamps and persist permanently in the feed. Transient actions like copying invite links or waiting for peer connection should be temporary (without timestamps) and cleaned up automatically once real session activity begins.
3. **In-Room Add Peer Feedback**: Clicking the header `[+]` button copies the invite link silently without clear inline feedback. The user wants a temporary `"Invite link copied to clipboard"` notice that automatically self-clears after 5 seconds, restarting the timer on repeated clicks.

---

## 2. Requirements & Acceptance Criteria

### REQ-033.1: Icon-Only Two-Stage Bottom Button
- The bottom button (`#btn-create-room`) must contain **no text labels** (remove `<span>` text elements).
- Flow consists of exactly **2 stages**:
  - **Stage 1 (Unclicked / Default)**:
    - Displays the **create chat symbol** (message bubble with `+` icon).
    - Uses the standard bright emerald styling (`.btn-emerald`).
    - Tooltip / `aria-label="Create Chat"`.
  - **Stage 2 (Clicked / Active Invite & Copy)**:
    - Upon clicking `#btn-create-room`, transitions immediately to display the **copy icon** (two overlapping rectangles).
    - Uses the darker shade of green (`.btn-waiting` styling).
    - Tooltip / `aria-label="Copy invite link"`.
    - If clicked again while in Stage 2, re-copies the existing invite link to clipboard and remains in Stage 2.
- When the chat session is reset, disconnected, or burned, the button returns to Stage 1.

### REQ-033.2: Temporary System Messages & Bottom Button Flow
- **Temporary System Message Definition**:
  - Rendered without a timestamp (`.system-timestamp` omitted).
  - Styled with a dedicated temporary class (e.g. `system-message system-temporary`).
- **Bottom Button Click Behavior**:
  - When `#btn-create-room` is clicked (both initial click and subsequent re-clicks), it clears any existing temporary messages and displays:
    1. `"Invite link copied to clipboard"` (temporary, no timestamp)
    2. `"Waiting for peer..."` (temporary, no timestamp, with animated ticking ellipsis `.ticking-dots`)
- **Clearing Contract**:
  - All temporary system messages are automatically removed from the DOM when the next permanent system message is appended (e.g., `"Joined chat"`, `"Peer joined chat"`, `"Peer left chat"`, `"Session burned"`, etc.).
  - Re-clicking the bottom button clears and re-displays these 2 temporary messages.

### REQ-033.3: Top Bar `[+]` Button 5-Second Self-Clearing Temporary Notice
- When the user clicks the top-bar Add Peer button (`#header-add-btn`):
  - Copies (or re-copies) the invite link to `navigator.clipboard`.
  - Displays a temporary `"Invite link copied to clipboard"` system message (without timestamp) with a subtle CSS pulse micro-animation (e.g. `@keyframes noticePulse`).
  - The message self-clears (is removed from the feed) automatically after **5,000ms (5 seconds)**.
  - **Re-click Feedback**: If the user clicks `#header-add-btn` again during the 5-second countdown, the existing temporary notice re-triggers the subtle pulse micro-animation (providing tactile visual feedback that the re-copy occurred) and restarts the 5-second countdown timer. The `[+]` button itself is left unchanged.

### REQ-033.4: Quality Gates & Automated Regression
- Update headless browser regression tests (`zero_client/tests/browser/test_mesh_scenario.js`) to assert:
  - Bottom button has 0 text content and transitions icon and classes between Stage 1 and Stage 2.
  - Temporary messages have no timestamp.
  - Permanent messages remove active temporary messages.
  - Re-clicking bottom button clears and re-emits temporary messages.
  - Header `[+]` click produces temporary message that disappears after 5 seconds, and restarts timer on re-click.
- Full workspace quality gates pass: `cargo fmt`, `cargo clippy`, `cargo test`, `wasm32` builds, and `run_test.sh`.

---

## 3. Scope Boundaries

- **In-Scope**:
  - `zero_client/index.html` (DOM structure, button states, temporary message functions, timer management).
  - `zero_client/style.css` (temporary system message styling, icon-only button styling adjustments).
  - `zero_client/tests/browser/test_mesh_scenario.js` (DOM assertions and unit tests).
- **Out-of-Scope**:
  - No changes to WebRTC signaling, `zero_protocol`, cryptographic envelope, or Cloudflare Workers.

---

## 4. Verification Criteria

| ID | Description |
|---|---|
| **V-033.1** | Verify `#btn-create-room` has no text and displays create chat SVG in Stage 1 and copy SVG in Stage 2 with darker green. |
| **V-033.2** | Verify clicking `#btn-create-room` displays `"Invite link copied to clipboard"` and `"Waiting for peer..."` with ticking dots and without timestamps. |
| **V-033.3** | Verify re-clicking `#btn-create-room` clears and re-emits both temporary messages. |
| **V-033.4** | Verify arrival of any permanent system message clears all active temporary messages. |
| **V-033.5** | Verify clicking `#header-add-btn` emits `"Invite link copied to clipboard"` without timestamp, self-clears after 5s, and re-clicking re-triggers the pulse animation and restarts the 5s timer without modifying the button itself. |
| **V-033.6** | Verify all headless browser mesh tests and cargo test suites pass cleanly. |
