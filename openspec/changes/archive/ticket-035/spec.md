# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-035"
title: "Enhanced Burn Behavior: 90s Ephemeral TTL with Aging Fade, Per-Message Distributed Burn, and Minimal Dissolve"
state: "COMPLETED"
created_at: "2026-10-24T16:15:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-035/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

`zero_chat` currently supports whole-session destruction (`GlobalBurnSession`), selective peer purge on disconnect (`PeerLeftAndBurn`), and temporary notice clearing. However, during an active conversation:
1. **Persistent History During Session**: Messages persist in the DOM indefinitely until the session is terminated or burned. The user's core philosophy is that conversation lives in the *here and now*—words are spoken and naturally dissolve into the air as conversation moves forward in time.
2. **Aging Fade & 90-Second Ephemeral Lifecycle**: Messages (both user chat bubbles and system messages) should naturally age and dissolve over 90 seconds. To provide an intuitive acoustic sense of time without hurting legibility, messages remain at 100% full contrast for 70 seconds, gently soften to 80% opacity between 70s and 90s (preserving strong >9:1 contrast), and smoothly dissolve away at 90s with a 200ms soft fade.
3. **Quiet Room Invariant**: When all messages have dissolved in an active session, the screen remains an open, serene, quiet space (never re-popping the welcome marketing splash cards).
4. **Per-Message Distributed Burn**: The user wants the ability to selectively retract any specific chat message immediately across the entire mesh. Each user chat bubble needs a subtle flame button in its top-right corner. Clicking it burns the message across all connected peers in real time with the 200ms soft dissolve.
5. **Chat Bubble Sizing & Burn Button Aesthetics**: Chat bubbles must have an expanded minimum size (`min-width: 135px; min-height: 44px;`) so the top-right burn button does not crowd or overlap timestamps and text. The button should have no border, a subtle gradient separating it from the bubble background, and hover to the flame-red accent color.

---

## 2. Requirements & Acceptance Criteria

### REQ-035.1: 90-Second Local Message Lifecycle with Aging Fade
- Every element appended to the chat feed (`.message-bubble` and `.system-message`) registers a client-side 90-second self-destruct lifecycle:
  - **0s – 70s**: Full brightness and contrast (`opacity: 1.0`).
  - **70s – 90s (Aging Fade)**: Transitions gently to `opacity: 0.80` via smooth CSS transition (`transition: opacity 20s linear;`), maintaining complete legibility (>9:1 contrast ratio) while visually signaling temporal aging.
  - **At 90s**: Triggers the 200ms minimal soft fade (`opacity: 0.80 -> 0; transform: scale(0.98);`), collapses height/margin over ~150ms, and removes the element from the DOM (`el.remove()`).
- Self-contained and client-local: does not require network packets or peer consensus.
- Timer hygiene: if a message is burned manually before 90s or the room is reset/left, its aging and removal timers are cleanly cancelled.
- **Quiet Room Invariant**: In an active room session (`isInRoomSession === true`), dissolving all messages leaves an empty chat view and never restores the welcome splash screen.

### REQ-035.2: Per-Message Distributed Burn Button on Chat Bubbles
- Every user chat bubble (`.message-bubble`, both outgoing and incoming) features a dedicated flame button in its top-right corner (`position: absolute; top: 6px; right: 8px;`).
- Uses the same flame SVG icon as the header session burn button (scaled to ~13-14px).
- Styling:
  - No button border (`border: none;`).
  - Subtle radial/linear gradient background against the bubble surface so the flame icon remains visible and distinct.
  - Subtle normal color (`color: rgba(255, 255, 255, 0.45);`).
  - Interactive cursor: `cursor: pointer;` on the flame button, transitioning on hover to the top burn button's red accent (`color: #fca5a5; background: rgba(239, 68, 68, 0.25); box-shadow: 0 0 10px rgba(239, 68, 68, 0.35);`).
  - Text selection and cursor on bubble content remain default browser behavior (standard I-beam on selectable text).
- Sizing:
  - Chat bubbles have an increased minimum size (`min-width: 135px; min-height: 44px;`) to ensure short messages (e.g. "ok", "hi") accommodate the dot, timestamp, and flame button cleanly without collision.

### REQ-035.3: In-Band Distributed Burn Signaling (`BurnMessage`)
- Each chat message element stores its unique message UUID (`data-message-id="<uuid>"`).
- When the user clicks the per-message burn button:
  1. The local bubble plays the 200ms soft fade animation and is removed from the DOM.
  2. The client broadcasts an in-band `BurnMessage` frame over all active WebRTC DataChannels:
     ```json
     {
       "frame_id": "<uuid>",
       "ttl": 3,
       "signal": {
         "BurnMessage": {
           "message_uuid": "<target_message_uuid>"
         }
       }
     }
     ```
- Remote recipient peers:
  - Deduplicate via `frame_id`.
  - Locate `.message-bubble[data-message-id="<target_message_uuid>"]`.
  - Trigger the 200ms soft fade animation and remove the matching bubble from the DOM.
  - Forward the `BurnMessage` frame across the mesh if TTL > 1 (Gossip flooding with loop suppression).

### REQ-035.4: Minimal Soft Fade CSS Animation
- Dedicated minimal CSS transition (e.g. `.message-dissolve`):
  - Fades opacity to 0 over 200ms with a subtle scale contraction (`scale(0.98)`).
  - Concurrently collapses `max-height` and `margin` smoothly so adjacent messages glide into place without abrupt jumping.
  - Applied uniformly to:
    - Chat bubbles burned manually via the flame button.
    - Chat bubbles burned remotely via `BurnMessage` signal.
    - Chat bubbles and system messages expiring via the 90-second TTL.

### REQ-035.5: Automated Headless Regression Tests & Quality Gates
- Update/extend headless browser regression suite (`zero_client/tests/browser/test_mesh_scenario.js`):
  - Test 15 covering:
    - Top-right burn button DOM existence and bubble minimum sizing.
    - Local burn button click triggers 200ms dissolve and purges the bubble.
    - Multi-peer mesh distributed message burn (peer A burns message; peer B and C receive `BurnMessage` and purge the bubble).
    - 70s aging transition and 90s auto-burn timer (using simulated mock environment timers).
    - Quiet room invariant (empty feed in active room does not pop welcome splash).
- All 123 Rust workspace tests pass.
- `cargo fmt` and `cargo clippy` pass with zero warnings.

---

## 3. Architecture & Affected Modules

- `zero_client/index.html`:
  - `appendMessageBubble`: attach `data-message-id`, render top-right flame button, register 70s aging timer & 90s auto-burn timer.
  - `appendSystemMessage` & `appendTemporarySystemMessage`: register 70s aging timer & 90s auto-burn timer.
  - In-band signal listener: handle `signal.BurnMessage`, query matching bubble, animate & remove, forward with TTL.
  - Burn action helper: broadcast `BurnMessage`, animate & remove local bubble.
  - `showWelcomeSplash`: enforce quiet room invariant (`if (isInRoomSession) return;`).
- `zero_client/style.css`:
  - Minimum bubble width/height constraints (`min-width: 135px; min-height: 44px;`).
  - Per-message flame button styling, pointer cursor (`cursor: pointer;`), gradient background, hover red accent.
  - `.message-aging` (opacity 0.8) and `.message-dissolve` (200ms soft exit with height collapse).
- `zero_client/tests/browser/test_mesh_scenario.js`:
  - Test 15 covering per-message burn, distributed sync, 90s TTL, aging fade, and quiet room.
