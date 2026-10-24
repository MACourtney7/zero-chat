# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-036"
title: "Limbo Recovery: In-Room Plus Invite Button on Peer Disconnect, Staggered Notice Flow, and Two-Stage Icon Morphing"
state: "IN_PROGRESS"
created_at: "2026-10-24T18:20:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-036/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

In `zero_chat`, when all peers disconnect from an active chat room, the remaining participant is left in an unrecoverable "limbo state":
1. **Disabled Bottom Input**: The bottom message bar (`#message-form`) remains visible with a disabled text input and disabled send button.
2. **Hidden Top-Bar Invite Button**: `#header-add-btn` in the top control bar is hidden upon peer departure (`headerAddBtn.classList.add("hidden")`), and the bottom `#create-room-bar` is also hidden.
3. **Trapped Without In-Room Invite Capability**: The remaining user cannot generate a new invite link to bring someone into the chat without completely exiting or burning their session, which prematurely destroys their existing chat feed.

### Desired Behavior
- In this disconnected in-room state:
  1. The disabled message input form (`#message-form`) is hidden and replaced with the full-width action button bar (`#create-room-bar`).
  2. The button (`#btn-create-room`) is rendered in a modified initial Stage 1: displaying a clean plus (`+`) icon (matching the top bar add-peer button SVG geometry), rather than the speech-bubble-plus icon used on the empty home screen.
  3. On click, the button transforms into the identical copy icon (`COPY_ICON_SVG`) and waiting state (`.btn-waiting`) as the create chat button.
  4. On click, the button generates/copies an invite link to the clipboard and triggers the same staggered system messages: Notice 1 (`"Invite link copied to clipboard"`) immediately, followed by Notice 2 (`"Waiting for peer..."` with `.ticking-dots`) after ~450ms.
  5. Subsequent clicks while waiting re-copy the invite link to the clipboard and restart the staggered notices cleanly.
  6. When a new peer connects, the bottom button bar is hidden, the chat text input form (`#message-form`) is restored and enabled for messaging, and `#header-add-btn` is restored.
  7. When the session is exited or burned (return to home screen), `#btn-create-room` restores to its default home view icon (`CREATE_CHAT_ICON_SVG`, speech bubble with +).

---

## 2. Requirements & Acceptance Criteria

### REQ-036.1: Limbo State Detection & Footer Input Replacement
- In `handlePeerDeparture(leavingPeerId)` when all peers have departed and no peers are connecting (`!anyConnecting` / `openPeerCount === 0` in an active session):
  - Hide the disabled `#message-form` (`messageForm.classList.add("hidden")`).
  - Display `#create-room-bar` (`createRoomBar.classList.remove("hidden")`).
  - Configure `#btn-create-room` in Stage 1 Plus mode:
    - CSS classes: `btn-action btn-action-full btn-emerald`.
    - SVG icon: Plus (`+`) icon with matching top-bar SVG geometry (`<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="5" x2="12" y2="19"></line><line x1="5" y1="12" x2="19" y2="12"></line></svg>`).
    - Accessibility: `aria-label="Add peer"`.

### REQ-036.2: Two-Stage Morphing & In-Room Invite Generation
- Clicking `#btn-create-room` while in this in-room disconnected state:
  - If a pending invite link is already active (`pendingInviteUrl`), re-copy it to `navigator.clipboard`.
  - If no pending invite is active, generate a fresh WebRTC peer offer via `engine.create_peer_offer(assignedPeerId)`, establish the signaling dead-drop WebSocket (`pendingInviteWs`), and copy the invite URL to `navigator.clipboard`.
  - Morph `#btn-create-room` into Stage 2:
    - CSS classes: `btn-action btn-action-full btn-waiting`.
    - SVG icon: Copy icon (`COPY_ICON_SVG`).
    - Accessibility: `aria-label="Copy invite link"`.

### REQ-036.3: Staggered Temporary System Notices
- On click (and re-clicks), trigger identical staggered feedback via `triggerCreateRoomNotices()`:
  - Stage 1 notice immediately: `"Invite link copied to clipboard"` (clearing prior temporary notices).
  - Stage 2 notice after 450ms: `"Waiting for peer"` with animated `.ticking-dots`.
  - Re-clicking while in Stage 2 cleanly restarts the 450ms timer, clears temporary notices, and displays the staggered notices again.

### REQ-036.4: Reconnection & Session Teardown Restoration
- When a new peer connects (`onStateChangeCb` or `onPeerStateChangeCb` becomes `"open"`):
  - Pending invite WebSocket is closed and cleaned up (`clearPendingInvite`).
  - `#create-room-bar` is hidden (`createRoomBar.classList.add("hidden")`).
  - `#message-form` is unhidden (`messageForm.classList.remove("hidden")`), and `#message-input` and `#send-button` are enabled.
  - Top-bar `#header-add-btn` is unhidden (`headerAddBtn.classList.remove("hidden")`).
- When the room is left (`headerLeaveBtn`) or destroyed (`destroy-session-button`):
  - `resetToHome()` restores `#btn-create-room` to default Stage 1 (`CREATE_CHAT_ICON_SVG`, speech bubble with +).

### REQ-036.5: Temporary System Message TTL Exemption
- Temporary system messages (`.system-temporary`, which omit timestamps, e.g. `"Invite link copied to clipboard"` and `"Waiting for peer"`):
  - Must be explicitly exempt from `registerEphemeralLifecycle(el)`.
  - They do not age-fade at 70s and do not dissolve at 90s. They remain visible until naturally cleared by UX flow (`clearTemporarySystemMessages()`, such as upon peer connection, permanent message arrival, or re-clicking the invite button).
- The special-case 5-second self-clearing message generated by `#header-add-btn` (`triggerHeaderAddFeedback`) retains its existing 5-second pulse timer behavior.
- Permanent system messages (with timestamps, e.g. `"Peer joined chat"`, `"Peer left chat"`) and user chat bubbles continue to participate in the 90s ephemeral lifecycle with 70s aging fade.

### REQ-036.6: Headless Browser Regression Tests & Quality Gates
- Extend `zero_client/tests/browser/test_mesh_scenario.js` with **Test 16**:
  - Connect a peer to host, then disconnect the peer.
  - Verify `#message-form` is hidden and `#create-room-bar` is visible with `+` icon.
  - Click `#btn-create-room`: verify clipboard copy, transition to copy icon (`COPY_ICON_SVG`), and staggered temporary notices ("Invite link copied to clipboard" + "Waiting for peer").
  - Verify temporary notices do not expire after 90s simulated time (`isWaitingDots` / `.system-temporary` elements remain in feed).
  - Re-click `#btn-create-room`: verify re-copy and restart of staggered notices.
  - Connect a new peer: verify `#create-room-bar` is hidden, `#message-form` is restored & enabled, temporary notices are cleared, and `#header-add-btn` is shown.
  - Disconnect and leave to home: verify `#btn-create-room` restores to `CREATE_CHAT_ICON_SVG`.
- All 123 Rust workspace tests pass (`cargo test --workspace`).
- `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` pass cleanly with zero warnings.

---

## 3. Architecture & Affected Modules

- `zero_client/index.html`:
  - Constant `ADD_PEER_ICON_SVG`: define the 20x20 `+` icon SVG.
  - Function `setCreateRoomStage1(mode = "default")` (or `setCreateRoomStage1Plus()`): supports switching between the default chat bubble icon and the `+` icon while maintaining `btn-emerald` styling.
  - Function `handleCreateRoomClick()`: checks whether the client is in an active room session (`isInRoomSession && engine`). If in-room, routes through in-room peer invite generation (`handleInRoomAddPeerClick`) while applying the full two-stage copy icon and staggered notices flow.
  - Function `handlePeerDeparture`: when `!anyConnecting` occurs, hides `messageForm`, unhides `createRoomBar`, and sets Stage 1 Plus mode.
  - Engine state transitions (`onStateChangeCb`, `onPeerStateChangeCb`): when a peer opens, hides `createRoomBar`, unhides `messageForm`, and restores `headerAddBtn`.
  - Function `resetToHome`: restores `createRoomBar` to default Stage 1 (`CREATE_CHAT_ICON_SVG`).
  - Function `appendTemporarySystemMessage`: removes call to `registerEphemeralLifecycle(el)`, ensuring temporary messages do not timeout at 90s.
- `zero_client/tests/browser/test_mesh_scenario.js`:
  - Add Test 16 testing the complete limbo recovery workflow and temporary notice TTL exemption.
