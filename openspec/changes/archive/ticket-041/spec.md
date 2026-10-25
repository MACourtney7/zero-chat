# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-041"
title: "Mobile Browser Support & Resilience: Viewport, Safe-Area Insets, Focus Retention, and Reconnection"
state: "COMPLETED"
created_at: "2026-10-25T13:55:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-041/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

`zero_chat` operates reliably on desktop browsers, but exhibits both visual clipping and functional lifecycle failures on mobile browsers (iOS Safari, Android Chrome, mobile Firefox, and in-app WebViews):

1. **iOS Status Bar / Notch Collision**: `<meta name="viewport" ... viewport-fit=cover>` allows the page to render underneath the iOS status bar (clock, battery, Wi-Fi), causing `.chat-header` to overlap system icons.
2. **Virtual Keyboard Displacing Header Off-Screen**: On iOS Safari, focusing `#message-input` slides up the virtual keyboard; Safari does not reduce `100vh` layout height, scrolling the entire page upward and pushing `.chat-header` and top messages off-screen.
3. **Horizontal Clipping & Auto-Zoom**: Font sizes below 16px on `#message-input` trigger iOS Safari's mandatory ~114% auto-zoom on focus, cropping left and right margins (~15–20px). Flex containers lacking `min-width: 0` overflow `100vw`.
4. **Keyboard Dismissal on Send**: Tapping `#send-button` shifts focus away from `#message-input`, closing the virtual keyboard after every single message sent.
5. **Mobile Clipboard User-Gesture Expiration (`NotAllowedError`)**: Generating an invite awaits non-trickle ICE gathering (up to 3000ms). Transient user activation expires during this await on mobile browsers, causing `navigator.clipboard.writeText(url)` to throw `NotAllowedError`. Currently, the error is swallowed and falsely reports `"Invite link copied to clipboard"`.
6. **Host WebSocket Drop & Joiner 404 on App Switch**: When the Host switches to a messaging app (WhatsApp, Signal, iMessage) to paste the invite URL, the mobile OS suspends or closes background WebSockets. When the Joiner opens the link, the Durable Object returns HTTP 404 (`HostSocketUnavailable`). If the Joiner immediately fails on 404, connection is aborted before the Host can switch back.
7. **Mobile ICE Gathering Latency**: Mobile cellular/Wi-Fi interfaces require additional time to wake the radio and receive STUN responses.
8. **Premature Session Burn on Tab Backgrounding**: `window.addEventListener("pagehide", sendOpportunisticLeave)` fires during system dialogs or app switching on iOS Safari even when the page is merely cached in the bfcache (`event.persisted === true`), falsely broadcasting `leave_and_burn` and incinerating the session.

---

## 2. Requirements & Acceptance Criteria

### REQ-041.1: Mobile Viewport, Safe-Area Insets & Virtual Keyboard Geometry
- In `zero_client/style.css`:
  - Lock `html, body` against unintended elastic scrolling:
    ```css
    html, body {
      position: fixed;
      inset: 0;
      width: 100%;
      height: var(--app-height, 100dvh);
      overflow: hidden;
      overscroll-behavior: none;
      -webkit-text-size-adjust: 100%;
    }
    #app {
      position: fixed;
      top: var(--app-offset-top, 0px);
      left: 0;
      right: 0;
      width: 100%;
      max-width: 100vw;
      height: var(--app-height, 100dvh);
      overflow: hidden;
      box-sizing: border-box;
    }
    ```
  - Apply top safe-area insets:
    ```css
    .chat-header {
      padding-top: calc(12px + env(safe-area-inset-top, 0px));
    }
    #landing-view, .welcome-splash {
      padding-top: env(safe-area-inset-top, 0px);
      padding-bottom: env(safe-area-inset-bottom, 0px);
    }
    ```
  - Apply bottom safe-area insets to `.message-dock` / `.message-form`:
    - Default (keyboard closed): `padding-bottom: calc(12px + env(safe-area-inset-bottom, 0px));`
    - When `.keyboard-open` class is present on `html`: `padding-bottom: 8px;` (eliminates gap above virtual keyboard).
- In `zero_client/index.html`:
  - Register `window.visualViewport` listeners for `resize` and `scroll`:
    - Update CSS variables `--app-height: ${vv.height}px` and `--app-offset-top: ${vv.offsetTop}px`.
    - Toggle `.keyboard-open` class on `document.documentElement` when `vv.height < window.innerHeight * 0.85`.
    - Prevent scroll drift with `window.scrollTo(0, 0)`.
    - Automatically maintain feed scroll pinned to bottom (`feed.scrollTop = feed.scrollHeight`) when keyboard opens.

### REQ-041.2: Horizontal Clipping Elimination & 16px Font Size Invariant
- In `zero_client/style.css`:
  - `#message-input`, `.message-input`:
    ```css
    #message-input,
    .message-input {
      font-size: 16px;
      min-width: 0;
      flex: 1 1 auto;
    }
    ```
    (Strictly `>= 16px` to disable iOS Safari mandatory focus zoom).
  - Enforce `box-sizing: border-box; width: 100%; max-width: 100%; overflow-x: hidden;` across `.chat-header`, `.global-burn-row`, `#chat-feed`, `.message-form`, and container panels.
  - Constrain message bubbles: `max-width: 82%; word-break: break-word; overflow-wrap: anywhere;` with touch scrolling `-webkit-overflow-scrolling: touch; overscroll-behavior: contain;`.

### REQ-041.3: Touch Interaction & Input Focus Retention on Send
- In `zero_client/index.html`:
  - Attach a `pointerdown` / `touchstart` listener on `#send-button`:
    ```javascript
    sendBtn.addEventListener("pointerdown", (e) => {
      e.preventDefault();
    });
    ```
    This prevents button click from taking focus and blurring `#message-input`.
  - In `doSendMessage()`, ensure focus is explicitly preserved on `#message-input` after message submission so the user can continue typing continuously on mobile keyboards without reopening the virtual keyboard.

### REQ-041.4: Mobile Clipboard User-Gesture Compliance & Honest Fallback
- In `zero_client/index.html`:
  - For asynchronous invite generation in `createRoomAsHost` and in-room invite click handlers:
    - If `window.ClipboardItem && navigator.clipboard && navigator.clipboard.write` is available, initiate the write synchronously inside the user-activation click handler using a `ClipboardItem` wrapping a deferred `Promise` resolving to `new Blob([url], { type: "text/plain" })`.
  - Honest Fallback on Clipboard Failure:
    - If both `ClipboardItem` and `navigator.clipboard.writeText` fail or throw `NotAllowedError`:
      - Do NOT emit `"Invite link copied to clipboard"`.
      - Store the generated URL in `activeHostInviteUrl` or `pendingInviteUrl`.
      - Morph button to Stage 2 (`.btn-waiting`, `COPY_ICON_SVG`).
      - Display honest temporary notice: `"Invite ready — tap Copy to copy link"`.
      - When user taps the Stage 2 button, `navigator.clipboard.writeText` executes synchronously within 0ms of the user gesture, succeeding 100% of the time across all mobile browsers and triggering `"Invite link copied to clipboard"`.

### REQ-041.5: Host WebSocket Suspension & Auto-Reconnection on App Switch
- In `zero_client/index.html`:
  - Store active waiting signaling context in memory:
    - `activeHostSignaling = { doId, hostSecretHex, assignedPeerId }` for host room creation.
    - `activeInviteSignaling = { doId, hostSecretHex, assignedPeerId }` for in-room secondary invites.
  - Listen to `visibilitychange` on `document`:
    - When `document.visibilityState === "visible"` (or if `hostWs`/`pendingInviteWs` fires unexpected `onclose` while still waiting for `assignedPeerId` to transition to `"open"`):
      - Re-open WebSocket to `${WORKER_WS_ORIGIN}/host/${doId}?secret=${hostSecretHex}`.
      - Re-bind message listener to receive joiner answers piped by the Durable Object.
  - Clear signaling context once peer channel opens or room session is reset.

### REQ-041.6: Joiner 404 Retry Loop on Delayed Host Return
- In `zero_client/index.html`:
  - In `joinRoomFromHash`: When Joiner executes `POST /drop/${invite.do_id}/${invite.invite_uuid}`:
    - If response status is HTTP 404 (`HostSocketUnavailable`, host switched to messaging app and hasn't returned yet):
      - Display transient status notice: `"Waiting for host..."`.
      - Retry `POST /drop/...` every 1500ms up to 10 attempts (~15 seconds).
      - If HTTP 401 (invalid HMAC/token) or 410 (burned), abort retry immediately with failure.
      - When Host returns to browser, WebSocket reconnects, Durable Object returns 200 OK, and WebRTC connection snaps open.

### REQ-041.7: Mobile ICE Gathering Latency & Bfcache False-Burn Prevention
- In `zero_client/index.html`:
  - In `_waitForIceGatheringComplete(pc)`: Increase fallback timeout limit from 3000ms to 4500ms to accommodate mobile cellular STUN latency. Ensure cleanup removes `icegatheringstatechange` and `icecandidate` listeners.
  - In `sendOpportunisticLeave`:
    - Update `pagehide` event listener:
      ```javascript
      window.addEventListener("pagehide", (event) => {
        if (!event.persisted) {
          sendOpportunisticLeave();
        }
      });
      ```
    - Prevents tab backgrounding/app-switching into bfcache from firing `sendOpportunisticLeave()` and burning active sessions prematurely.

### REQ-041.8: Automated Headless Mobile & Regression Test Suite
- In `zero_client/tests/browser/test_mesh_scenario.js`:
  - Implement Test 18 (Mobile Viewport, Virtual Keyboard & Reconnection):
    - Assert `document.documentElement.scrollWidth <= window.innerWidth` across landing view, chat view with long text, and notices.
    - Assert `getComputedStyle(document.getElementById('message-input')).fontSize === '16px'`.
    - Simulate `visualViewport` height resizing (keyboard open) and verify `.chat-header` top offset remains `>= 0` and `.input-dock` padding adapts.
    - Verify `sendBtn` `pointerdown` preventDefault behavior.
    - Verify Joiner 404 retry loop when host WebSocket drops and reconnects on visibility resume.
    - Verify `pagehide` with `event.persisted === true` does NOT trigger opportunistic leave.
  - Ensure all 123 Rust workspace tests pass (`cargo test --workspace`).
  - Dual WASM checks pass (`zero_client` and `zero_network`).
  - All 18 headless browser tests pass (`bash zero_client/tests/browser/run_test.sh`).

---

## 3. Target Architecture & Affected Modules

| Module / File | Scope & Responsibilities |
|---|---|
| `zero_client/style.css` | Safe-area inset paddings (`env(safe-area-inset-top/bottom)`), fixed viewport scroll lock (`--app-height`, `--app-offset-top`), 16px font-size on `#message-input`, overflow guards, and `.keyboard-open` padding adjustment. |
| `zero_client/index.html` | `visualViewport` sync handler, `#send-button` pointerdown focus preservation, `ClipboardItem` async promise copy with honest fallback, host WebSocket auto-reconnect on `visibilitychange`, Joiner 404 retry loop, 4500ms ICE gathering fallback, and `!event.persisted` check on `pagehide`. |
| `zero_client/tests/browser/test_mesh_scenario.js` | Test 18 automated headless assertions for mobile viewport dimensions, font size invariant, visualViewport adaptation, Joiner 404 retry, and bfcache persistence. |
| `openspec/specs/overview.md` | Reconcile canonical spec with Mobile Browser Support Invariant 17 upon task completion. |

---

## 4. Verification & Quality Gates

1. **Gate 1: Rust Unit & Integration Tests**: `cargo test --workspace` (123 / 123 tests pass).
2. **Gate 2: Dual WASM Compilation**:
   - `cargo check --target wasm32-unknown-unknown -p zero_client`
   - `cargo check --target wasm32-unknown-unknown -p zero_network`
3. **Gate 3: Formatting & Linting**:
   - `cargo fmt -- --check`
   - `cargo clippy --workspace --all-targets -- -D warnings`
4. **Gate 4: Headless Browser Regression Suite**:
   - `bash zero_client/tests/browser/run_test.sh` (18 / 18 tests pass).
5. **Gate 5: Architectural & Security Review**: Reviewer verification with `APPROVE` verdict.
