# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-034"
title: "UI Refinements: Zero Placeholder, Staggered Notice Timing, Ellipsis Fix and Disconnected Indicator"
state: "IN_PROGRESS"
created_at: "2026-10-24T15:25:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-034/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

In pursuit of a clean zero-text UI outside of system messages and chat messages:
1. **Residual Input Placeholder**: When all remote peers disconnect and the user remains in the empty room, `handlePeerDeparture` assigns `msgInput.placeholder = "Peer left chat"`. The user wants this placeholder removed completely so the input box remains permanently blank in all states.
2. **Synchronous Temporary Message Clutter**: Clicking `#btn-create-room` currently renders both `"Invite link copied to clipboard"` and `"Waiting for peer..."` simultaneously on the exact same tick. The user desires a staggered 1, 2 delivery (e.g. Message 1 immediately, followed by Message 2 after a brief rhythmic delay of ~450ms) both on initial click and upon subsequent re-clicks.
3. **Double Ellipsis Bug**: The temporary message was passing `"Waiting for peer..."` (literal 3 dots) while also appending `<span class="ticking-dots"></span>` (which appends `.`, `..`, `...` via CSS `::after`). This caused visual duplication: `"Waiting for peer...   ..."`. The base text must be `"Waiting for peer"` without dots so the ticking animation renders smoothly as `"Waiting for peer." -> "Waiting for peer.." -> "Waiting for peer..."`.
4. **Home View Slashed Person Icon vs. Empty Chat Counter**: In the header badge (`#peer-count`), while in the **home view** (initial state before room creation, with welcome splash visible), the badge should display a **slashed person icon** (user/person silhouette with a diagonal slash to indicate "no peers", in default blue / `--accent-cyan` styling). Once the user clicks the create chat button and transitions into the empty chat view (or joins a room), the peer counter transitions to display its current numeric value: `0` (and `1`, `2`, ... as peers connect). Upon returning to the home view (`resetToHome()`), it resets to the slashed person icon.

---

## 2. Requirements & Acceptance Criteria

### REQ-034.1: Permanent Removal of Input Placeholder
- Eliminate `msgInput.placeholder = "Peer left chat"` in `handlePeerDeparture`.
- Ensure `#message-input` has `placeholder=""` in all states (initial disconnected, active chat, peer departed, and reset).
- Update existing browser test assertions from `msgInput.placeholder === "Peer left chat"` to `msgInput.placeholder === ""`.

### REQ-034.2: Staggered Delivery for Room Creation Temporary Messages
- When `#btn-create-room` is clicked (both initial click in Stage 1 and re-clicks in Stage 2):
  1. Clear any existing temporary messages and cancel any in-flight stagger timers.
  2. Display notice 1 immediately: `"Invite link copied to clipboard"`.
  3. After a brief stagger delay (~450ms), append notice 2: `"Waiting for peer"` (with ticking ellipsis).
- If the user re-clicks `#btn-create-room` in Stage 2:
  - Cancel any pending second notice timer.
  - Clear existing temporary messages.
  - Re-render notice 1 immediately, followed by notice 2 after the stagger delay.
- If a permanent system message or peer connection arrives before notice 2 renders:
  - Cancel the pending stagger timer and purge all temporary messages.

### REQ-034.3: Fix Double Ellipsis in "Waiting for peer" Notice
- Update the temporary notice invocation from `appendTemporarySystemMessage("Waiting for peer...", true)` to `appendTemporarySystemMessage("Waiting for peer", true)`.
- Ensure the DOM text node contains strictly `"Waiting for peer"` without trailing dots.
- The ticking ellipsis `.ticking-dots` (`::after`) animates 0 to 3 dots cleanly without duplicate punctuation.

### REQ-034.4: Home View Slashed Person Icon & Empty Chat Numeric Counter
- **Home View State**:
  - While disconnected in the home view (e.g. initial page load or after `resetToHome()`), `#peer-count` displays a clean SVG **slashed person icon** (user/person silhouette with a diagonal slash indicating "no peers", in default blue `var(--accent-cyan)`).
- **In-Room State Transition**:
  - As soon as the user clicks `#btn-create-room` (or joins a room from a hash fragment) and enters the chat view, `#peer-count` transitions to displaying its numeric value: `"0"`.
  - When peers connect, `#peer-count` displays `"1"`, `"2"`, etc.
  - When peers disconnect and 0 peers remain in an active room, `#peer-count` truthfully displays `"0"`.
- **Return to Home**:
  - When returning to the home view via `#destroy-session-button`, `#header-leave-btn`, or `resetToHome()`, `#peer-count` restores the slashed person SVG icon.

### REQ-034.5: Quality Gates & Automated Regression
- Update/extend headless browser regression suite (`zero_client/tests/browser/test_mesh_scenario.js`):
  - Assert `msgInput.placeholder === ""` after peer departure.
  - Assert staggered delivery timing and sequence of the two notices.
  - Assert `"Waiting for peer"` text content has no trailing literal dots.
  - Assert `#peer-count` renders slashed person icon SVG in home view, `"0"` upon room creation, and numeric counts (`"1"`, etc.) when connected.
- All existing 123 Rust tests and browser regression tests pass 100%.

---

## 3. Architecture & Affected Modules

- `zero_client/index.html`:
  - `handlePeerDeparture`: set `msgInput.placeholder = ""`
  - `updatePeerCountDisplay`: render disconnected symbol when `count === 0`
  - `createRoomAsHost` & re-copy click handler: implement staggered delivery with timer management
  - Fix string literal from `"Waiting for peer..."` to `"Waiting for peer"`
- `zero_client/tests/browser/test_mesh_scenario.js`:
  - Update Test 11 line 482 assertion for placeholder
  - Update / add assertions for REQ-034.1 - REQ-034.4
