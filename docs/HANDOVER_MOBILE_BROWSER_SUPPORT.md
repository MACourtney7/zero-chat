# Architectural Handover Brief: Mobile Browser Support & Resilience (ticket-033)

## 1. Executive Summary & Objective
ZeroChat currently functions across desktop browsers (including multi-user mesh snapping and selective departure purging resolved in ticket-032), but fails both visually and functionally on mobile browsers (iOS Safari, Android Chrome, and in-app WebViews).

This handover brief defines the complete scope for Mobile Browser Support, covering:
- Mobile Viewport, Safe-Area, and Virtual Keyboard Geometry (`zero_client/style.css` & `zero_client/index.html`)
- Mobile Touch Interactions & Focus Retention (`zero_client/index.html`)
- Mobile OS App-Switching Lifecycle, Clipboard Gesture Compliance, & Signaling Reconnection (`zero_client/index.html`)

**Strategic Sequencing Note**: Do not migrate the inline JS runtime to pure Rust/WASM during this ticket. Stabilize all mobile UI and lifecycle mechanics in `zero_client/index.html` and `zero_client/style.css` first so the product behavior is 100% verified before the Rust/WASM consolidation pass.

---

## 2. Target File Scope
- `zero_client/style.css` (Primary CSS layout, safe-area insets, flex overflow guards, overscroll locking)
- `zero_client/index.html` (Viewport sync via `window.visualViewport`, synchronous clipboard fallback, WebSocket auto-reconnect on `visibilitychange`, dead-drop retry loop, and input focus retention)
- `zero_client/tests/browser/test_mesh_scenario.js` (Automated regression suite including mobile viewport, touch, and lifecycle emulation)

---

## 3. Root-Cause Diagnosis & Required Engineering Fixes

### Part A: Mobile UI, Viewport & Virtual Keyboard Geometry

1. **iOS Status Bar / Notch Collision (`env(safe-area-inset-top)`)**
   - **Symptom**: Because `<meta name="viewport" ... viewport-fit=cover>` is enabled, the webpage extends behind the iOS status bar (clock, cellular/Wi-Fi/battery icons). Currently, `.chat-header` renders at `y = 0px`, directly overlapping the iOS system icons.
   - **Required Fix (`zero_client/style.css`)**:
     Apply top safe-area inset padding to `.chat-header` and `#landing-view`:
     ```css
     .chat-header {
       padding-top: calc(12px + env(safe-area-inset-top, 0px));
     }
     #landing-view {
       padding-top: env(safe-area-inset-top, 0px);
       padding-bottom: env(safe-area-inset-bottom, 0px);
     }
     ```

2. **Virtual Keyboard Pushing `.chat-header` Off-Screen (`window.visualViewport`)**
   - **Symptom**: When tapping `#message-input` on iOS Safari, the virtual keyboard slides up. Because iOS Safari does not shrink 100vh/100dvh layout height for the software keyboard, it scrolls the entire document window upward—pushing `.chat-header`, `.global-burn-row`, and top messages completely off the top of the screen.
   - **Required Fix (`zero_client/index.html` & `zero_client/style.css`)**:
     In `zero_client/style.css`: Lock `html, body` from scrolling and bind `#app` height and top offset to CSS custom properties driven by `window.visualViewport`:
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
     In `zero_client/index.html`: Register a `window.visualViewport` listener on resize and scroll that updates `--app-height`, `--app-offset-top`, toggles a `.keyboard-open` class when `vv.height < window.innerHeight * 0.85`, prevents window scroll drift (`window.scrollTo(0, 0)`), and keeps `#message-list` scrolled to the latest message:
     ```javascript
     function syncMobileViewport() {
       const vv = window.visualViewport;
       if (!vv) return;
       document.documentElement.style.setProperty("--app-height", `${vv.height}px`);
       document.documentElement.style.setProperty("--app-offset-top", `${vv.offsetTop}px`);
       const isKeyboardOpen = vv.height < window.innerHeight * 0.85;
       document.documentElement.classList.toggle("keyboard-open", isKeyboardOpen);
       window.scrollTo(0, 0);
       if (messageList) {
         messageList.scrollTop = messageList.scrollHeight;
       }
     }
     if (window.visualViewport) {
       window.visualViewport.addEventListener("resize", syncMobileViewport);
       window.visualViewport.addEventListener("scroll", syncMobileViewport);
       syncMobileViewport();
     }
     ```
     **Bottom Safe-Area Adaptation**: When the keyboard is closed, `.input-dock` should respect `padding-bottom: calc(12px + env(safe-area-inset-bottom, 0px))`. When `.keyboard-open` is active on `html`, reduce `.input-dock` `padding-bottom` to `8px` so there is no dead gap above the keyboard.

3. **Horizontal Clipping & iOS Safari Auto-Zoom on Input Focus**
   - **Symptom**: System notices ("Invite link copied to clipboard") and message bubble borders are clipped on the left and right edges of narrow mobile screens (375px–390px).
   - **Root Causes**:
     - `#message-input` has a font-size under 16px, which triggers iOS Safari's mandatory auto-zoom (~114%) when focused, cropping ~15–20px off the left and right edges of the viewport.
     - Flexbox children without `min-width: 0` expand past `100vw`.
   - **Required Fix (`zero_client/style.css`)**:
     - Set `#message-input { font-size: 16px; min-width: 0; flex: 1 1 auto; }` (strictly >= 16px to disable iOS focus auto-zoom).
     - Enforce `box-sizing: border-box; width: 100%; max-width: 100%; overflow-x: hidden;` across `.chat-header`, `.global-burn-row`, `#message-list`, and `.input-dock`.
     - Give `#message-list` explicit horizontal padding (`padding: 12px 16px; -webkit-overflow-scrolling: touch; overscroll-behavior: contain;`) and constrain `.message` bubbles to `max-width: 82%; word-break: break-word; overflow-wrap: anywhere;`.

---

### Part B: Mobile Touch Interactions & Focus Retention

4. **Keep Virtual Keyboard Open When Tapping Send (`#send-btn`)**
   - **Symptom**: On mobile browsers, tapping `#send-btn` shifts focus from `#message-input` to the button, causing the virtual keyboard to close after every single sent message.
   - **Required Fix (`zero_client/index.html`)**:
     Attach a `pointerdown` / `touchstart` listener on `#send-btn` that calls `e.preventDefault()` so tapping Send does not blur `#message-input`, and explicitly keep focus on `messageInput` inside `doSendMessage()` if the user was typing.

5. **Mobile Clipboard User-Gesture Expiration (`NotAllowedError`)**
   - **Symptom**: In `createRoomAsHost()` and in-room invite generation, `navigator.clipboard.writeText(inviteUrl)` is called after `await engine.create_peer_offer(...)` (which awaits up to 3,000ms for non-trickle ICE gathering). On iOS Safari and mobile Chrome, transient user activation expires during the 1–3s await, causing `navigator.clipboard.writeText` to throw `NotAllowedError`.
   - **Required Fix (`zero_client/index.html`)**:
     - Use `ClipboardItem` with a `Promise` where supported (iOS Safari 13.1+ standard for async clipboard writes): Inside the synchronous click handler stack (before the first await), if `window.ClipboardItem && navigator.clipboard && navigator.clipboard.write` is available, initiate the clipboard write with a `ClipboardItem` wrapping a `Promise` that resolves to a `Blob([inviteUrl], { type: "text/plain" })` once `inviteUrl` is built.
     - **Honest Fallback Notice**: If both `ClipboardItem` and `navigator.clipboard.writeText(inviteUrl)` fail (throw `NotAllowedError`), do not emit "Invite link copied to clipboard" on that first tap. Instead, transition the button into Stage 2 (`activeHostInviteUrl` ready) and display "Invite ready — tap Copy to copy link". When the user taps the Stage 2 button (where `pendingInviteUrl` is set), `navigator.clipboard.writeText` executes synchronously within 0ms of the tap, which succeeds 100% of the time on all mobile browsers and triggers feedback notices.

---

### Part C: Mobile App-Switching Lifecycle & Signaling Reconnection

6. **Host WebSocket Suspension & Reconnection on App Switch (`visibilitychange` / `onclose`)**
   - **Symptom**: On a phone, the Host creates an invite link, copies it, and switches to WhatsApp/iMessage/Signal to paste it to a friend. The moment Safari/Chrome enters the background, the mobile OS suspends or closes `hostWs` (`wss://.../host/${doId}?secret=${hostSecretHex}`). When the Joiner taps the link, the Cloudflare Durable Object finds 0 connected host sockets and returns HTTP 404 (`HostSocketUnavailable`).
   - **Required Fix (`zero_client/index.html`)**:
     - **Persist Active Signaling Context in Memory**: Store the active waiting signaling state (`activeHostSignaling = { doId, hostSecretHex, assignedPeerId }` for initial room creation, and `activeInviteSignaling = { doId, hostSecretHex, assignedPeerId }` for in-room add peer).
     - **Auto-Reconnect Host WebSocket**: If `hostWs` or `pendingInviteWs` closes unexpectedly (`onclose`) or if `document.visibilityState === "visible"` fires while the Host is still waiting for that `assignedPeerId` to transition to `"open"`, automatically re-open the WebSocket to `${WORKER_WS_ORIGIN}/host/${doId}?secret=${hostSecretHex}` and re-bind `onmessage`.
     - **Joiner 404 Retry Loop (`joinRoomFromHash`)**: When a Joiner sends `POST /drop/${invite.do_id}/${invite.invite_uuid}`, if the response status is 404 (meaning the Host is still switching back from their messaging app to their mobile browser), do not fail immediately. Retry the `POST` every 1500ms (up to 10 attempts / ~15 seconds) while displaying a status notice ("Waiting for host...") until the Host's WebSocket reconnects and returns 200 OK (or stop immediately on 401/410).

7. **Mobile ICE Gathering Resilience (`_waitForIceGatheringComplete`)**
   - **Symptom**: Mobile cellular/Wi-Fi interfaces often take slightly longer to wake the radio and receive UDP STUN (srflx) responses.
   - **Required Fix (`zero_client/index.html`)**:
     In `_waitForIceGatheringComplete(pc)`, increase the fallback timeout from 3000ms to 4500ms on mobile (heuristic or globally), and properly remove the `icecandidate` and `icegatheringstatechange` listeners inside `cleanup()`.

8. **Prevent False `pagehide` Burn When Switching Apps on Mobile**
   - **Symptom**: Currently, `window.addEventListener("pagehide", sendOpportunisticLeave)` is registered alongside `beforeunload`. On some mobile browsers, `pagehide` or visibility transitions can fire during system dialogs or tab switches.
   - **Required Fix (`zero_client/index.html`)**:
     Ensure `sendOpportunisticLeave` only fires on actual document unload (`beforeunload`, or `pagehide` when `!event.persisted`), and never on `visibilitychange` (tab backgrounding).

---

## 4. Verification & Acceptance Criteria
- **Gate 1: Automated Workspace & Regression Suite**
  - `cargo test --workspace` (all Rust tests green)
  - `cargo check --target wasm32-unknown-unknown -p zero_client && cargo check --target wasm32-unknown-unknown -p zero_network`
  - `cargo fmt -- --check && cargo clippy --workspace --all-targets -- -D warnings`
- **Gate 2: Headless Mobile Viewport & Lifecycle Emulation Tests**
  - Automated headless checks for:
    - No Horizontal Overflow across views
    - `#message-input` font-size invariant (16px)
    - `visualViewport` resize simulation (virtual keyboard open keeps `.chat-header` top >= 0 and `.input-dock` bottom constrained)
    - Host WebSocket drop & Joiner 404 retry simulation
- **Gate 3: Deployment & Live Mobile Smoke Test**
