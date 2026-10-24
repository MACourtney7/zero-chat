# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-037"
title: "UI Architecture Refinement: Unified Header & Footer State Rules, Minimalist Top-Bar Badges, Clean Burn Icon, Timestamp Removal, and Joiner Message Integrity"
state: "IN_PROGRESS"
created_at: "2026-10-24T20:30:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-037/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

Through comprehensive user experience testing across room creation, peer joins, limbo disconnections, and connection failures, several inconsistencies and opportunities for aesthetic refinement were identified:

1. **Header & Footer Visibility Inconsistencies**:
   - In empty chat views (waiting for peers, limbo state, or connection failure), the Leave button (`#header-leave-btn`) was frequently missing, trapping users.
   - The bottom action button frequently appeared in the wrong visual variant (e.g. chat-bubble-plus instead of plus icon, or remained hidden for 15–30s during connection failures).
   - The user has established four clear, invariant UI rules to govern the entire client lifecycle:
     - **Rule 1**: If the peer counter is visible (numeric value `0`, `1`, `2`... in any room session), the Leave button (`#header-leave-btn`) MUST also be visible.
     - **Rule 2**: If the peer counter value is `0` (waiting for peer, limbo disconnect, or connection failure), the bottom action button (`#create-room-bar`) MUST be visible in the **plus icon variant** (`setCreateRoomStage1Plus()`), and the message input form (`#message-form`) must be hidden.
     - **Rule 3**: If the peer counter value is `> 0`, the `+` button (`#header-add-btn`) MUST be visible in the top bar, the message input form (`#message-form`) must be enabled and visible, and the bottom action bar (`#create-room-bar`) must be hidden.
     - **Rule 4**: The "new chat" version of the bottom button (`CREATE_CHAT_ICON_SVG`, chat bubble with plus) MUST ONLY be used as part of the home view (`resetToHome()`).
2. **Minimalist Top-Bar Badges & Brand Icon**:
   - The top-bar badges (`#peer-count`, `#idle-status-capsule`, `.peer-pill` lock capsule) currently render with pill borders and translucent background fills, creating visual clutter.
   - The brand icon (`.brand-icon`) on the left of the top bar has a gradient background and border around the `0` glyph.
   - **Intent**: Only interactive top-bar buttons (`#header-add-btn`, `#header-leave-btn`) should have a background and border. All badges (`.peer-count-badge`, `#idle-status-capsule`, `.peer-pill`) must have transparent backgrounds and no borders. The brand icon `.brand-icon` must remove its back/border so only the crisp cyan `0` glyph renders beside `zero-chat.pages.dev`.
3. **Streamlined Welcome Splash Messaging**:
   - The welcome splash contains the heading `"Zero accounts. Zero servers. Zero history."` and subtitle `"Create Chat -> Share Link -> Talk P2P"`.
   - **Intent**: Remove `"Zero accounts. Zero servers. Zero history."` from the splash. Change `"Create Chat -> Share Link -> Talk P2P"` to `"Create -> Share -> Chat"`.
4. **Circular Glow Around Message Burn Icon**:
   - In message bubbles, `.btn-message-burn` has a radial-gradient background and on hover exhibits a glowing red circle (`box-shadow: 0 0 10px rgba(239, 68, 68, 0.35)`).
   - **Intent**: Remove all circular backgrounds and glow. Only the SVG flame icon should be visible against the native bubble background color, shifting to `#fca5a5` on hover.
5. **Delayed Connection Failure Recovery**:
   - When joining via invite fails, an unclosed `RTCPeerConnection` kept gathering ICE for 15–30s in the background before belatedly unhiding the bottom bar.
   - **Intent**: Immediately close and abort engine resources upon failure, immediately display the Leave button in the header, and immediately display the bottom button in the plus icon variant with zero delay.
6. **Timestamp Removal**:
   - Timestamps are currently displayed alongside system messages (`.system-timestamp`) and inside chat bubbles (`.message-timestamp`).
   - **Intent**: Completely remove timestamps from display across both system messages (the two types—permanent and temporary—remain) and chat message bubbles.
7. **"Joined chat" System Message Integrity**:
   - A `"Joined chat"` system message must strictly and only be displayed when a client joins a chat via an invite link.
   - A host must never display `"Joined chat"`.
   - A joining client must only display `"Joined chat"` once upon initial connection; subsequent peer connections must trigger `"Peer joined chat"`.
   - A single peer connection must never trigger both `"Joined chat"` and `"Peer joined chat"` on the same client.

---

## 2. Requirements & Acceptance Criteria

### REQ-037.1: Unified Header & Footer State Machine
In `zero_client/index.html`, establish consistent reactive UI synchronization:
- **Home View (`!inRoom`)**:
  - Peer badge (`#peer-count`) displays slashed-person icon (`SLASHED_PERSON_ICON_SVG`).
  - Leave button (`#header-leave-btn`) is **hidden**.
  - Top `+` button (`#header-add-btn`) is **hidden**.
  - Message input (`#message-form`) is **hidden**.
  - Bottom action bar (`#create-room-bar`) is **visible** in New Chat mode (`setCreateRoomStage1()`, chat bubble with plus).
- **In-Room with 0 Peers (`inRoom && openPeerCount === 0`)**:
  - Peer badge (`#peer-count`) displays numeric string `"0"`.
  - Status capsule (`#idle-status-capsule`) displays blue dot.
  - Leave button (`#header-leave-btn`) is **visible** (`classList.remove("hidden")`).
  - Top `+` button (`#header-add-btn`) is **hidden**.
  - Message input (`#message-form`) is **hidden**.
  - Bottom action bar (`#create-room-bar`) is **visible** in Plus mode (`setCreateRoomStage1Plus()`, 20x20 plus icon). On click, transitions to Stage 2 waiting state (`.btn-waiting`, copy icon).
- **In-Room with >0 Peers (`inRoom && openPeerCount > 0`)**:
  - Peer badge (`#peer-count`) displays numeric string (`"1"`, `"2"`...).
  - Leave button (`#header-leave-btn`) is **visible**.
  - Top `+` button (`#header-add-btn`) is **visible** (`classList.remove("hidden")`).
  - Message input (`#message-form`) is **visible** and enabled.
  - Bottom action bar (`#create-room-bar`) is **hidden**.
- **Home Button Scope**:
  - `setCreateRoomStage1()` (chat bubble with plus) is strictly restricted to `resetToHome()`. All other 0-peer room views (host waiting, join failure, limbo disconnect) use `setCreateRoomStage1Plus()` (plus icon variant).

### REQ-037.2: Minimalist Top-Bar Badges & Brand Icon
- In `zero_client/style.css`:
  - **Brand Icon (`.brand-icon`)**: Remove gradient background and border (`background: transparent; border: none;`). Only the cyan `0` character displays.
  - **Badges (`.peer-count-badge`, `#peer-count`, `.status-capsule-disconnected`, `#idle-status-capsule`, `.peer-pill`)**:
    - Remove backgrounds and borders (`background: transparent; border: none;`).
    - Retain inner icon / text rendering (cyan font, slashed person icon, blue status dot, cyan lock icon).
  - **Top-Bar Buttons (`#header-add-btn`, `#header-leave-btn`)**:
    - Retain button backgrounds, borders, and hover micro-animations.

### REQ-037.3: Streamlined Welcome Splash
- In `zero_client/index.html` (initial DOM and `WELCOME_SPLASH_HTML` constant):
  - Remove `<h2 class="welcome-title">Zero accounts. Zero servers. Zero history.</h2>`.
  - Replace `<p class="welcome-subtitle">Create Chat -&gt; Share Link -&gt; Talk P2P</p>` with `<p class="welcome-subtitle">Create -&gt; Share -&gt; Chat</p>`.

### REQ-037.4: Clean Message Burn Button (No Background Circle or Glow)
- In `zero_client/style.css`, `.btn-message-burn`:
  - Default state: `background: transparent; border: none; box-shadow: none;`.
  - Hover state (`.btn-message-burn:hover`): `background: transparent; box-shadow: none; color: #fca5a5; transform: scale(1.1);`.
  - Active state (`.btn-message-burn:active`): `background: transparent; box-shadow: none; color: #f87171; transform: scale(0.95);`.
  - Only the SVG flame icon is visible against the background colour of the chat bubble.

### REQ-037.5: Immediate Connection Failure Recovery (Zero 15–30s Wait)
- In `zero_client/index.html` `joinRoomFromHash`:
  - If dead-drop fetch fails, or connection fails before opening:
    - Immediately terminate/close the active engine (`engine.close()`) so that background ICE gathering ceases immediately.
    - Clear active transient status message (`clearTransientStatus()`).
    - Append `"Failed to establish connection"`.
    - Apply In-Room 0-Peer UI rules immediately:
      - Leave button `#header-leave-btn` is visible in the top bar.
      - Bottom button `#create-room-bar` is immediately visible in Stage 1 Plus mode (`setCreateRoomStage1Plus()`).
      - `#message-form` is hidden.
    - Set `isJoining = false`.

### REQ-037.6: Display Timestamp Removal Across All Messages
- In `zero_client/index.html` and `zero_client/src/ui.rs`:
  - **System Messages**: Remove `<span class="system-timestamp">` creation and display from `appendSystemMessage()`. System messages render solely with `<span class="system-text">` (and optional `.ticking-dots` for temporary notices).
  - **Chat Message Bubbles**: Remove `<span class="message-timestamp">` creation and display from `appendMessageBubble()` in `index.html` and `create_message_element()` in `ui.rs`. Bubble header renders peer dot / self dot without a timestamp.
  - Retain the two functional classes of system messages: permanent messages (subject to 90s auto-burn) and temporary messages (exempt from auto-burn, cleared on UX transitions).

### REQ-037.7: "Joined chat" Message Integrity
- In `zero_client/index.html`:
  - `"Joined chat"` is strictly emitted ONLY when a joiner client successfully establishes its initial data channel to the room (`isJoining === true`).
  - Immediately upon emitting `"Joined chat"`, set `isJoining = false`.
  - A host creating a room never emits `"Joined chat"`.
  - Subsequent peer connections (via mesh introduction or in-room invite) always emit `"Peer joined chat"`.
  - Ensure `announcedJoinPeers` suppresses duplicate announcements so that the initial connection never emits both `"Joined chat"` and `"Peer joined chat"` on the same client.

### REQ-037.8: Automated Regression Test Suite (Test 17)
- Add Test 17 in `zero_client/tests/browser/test_mesh_scenario.js`:
  1. Verify `.brand-icon` and top-bar badges (`.peer-count-badge`, `.status-capsule-disconnected`, `.peer-pill`) have transparent background and no border.
  2. Verify welcome splash displays `"Create -> Share -> Chat"` and omits `"Zero accounts. Zero servers. Zero history."`.
  3. Verify `.btn-message-burn` has `background: transparent` and `box-shadow: none` on default and hover.
  4. Verify that no `.system-timestamp` or `.message-timestamp` elements exist in the feed or bubbles.
  5. Verify top/bottom bar rules across states:
     - Home: peer counter has slashed-person icon, leave button hidden, bottom button has chat-bubble icon.
     - Host waiting (0 peers): peer counter has "0", leave button visible, bottom button has plus icon.
     - Active chat (1 peer): peer counter has "1", leave button visible, top `+` button visible, message input visible.
     - Peer disconnect (0 peers / limbo): peer counter has "0", leave button visible, bottom button has plus icon.
     - Failed connection (0 peers): leave button visible, bottom button immediately visible with plus icon.
  6. Verify Joiner sees `"Joined chat"` exactly once and does not see `"Peer joined chat"` for that same initial host connection.
  7. Verify clicking Leave button returns client to home view across all states.

### REQ-037.9: Splash Screen Default Blue Vector Icons
- In `zero_client/index.html` (initial `#welcome-splash` DOM and `WELCOME_SPLASH_HTML` template):
  - Replace platform emojis (⚡, 🔒, 🛡️) with crisp, uniform inline SVG vector icons (lightning bolt polygon, padlock rect/path, shield path).
- In `zero_client/style.css`:
  - Style `.feature-icon` and `.feature-icon svg` in default blue (`color: var(--accent-blue); stroke: var(--accent-blue);`), replacing disparate multi-color emojis (yellow lightning, gold lock, red shield) with a uniform default blue appearance.

---

## 3. Target Architecture & Affected Modules

| Module / File | Changes & Responsibilities |
|---|---|
| `zero_client/style.css` | 1. Remove background and border from `.brand-icon`, `.peer-count-badge`, `#peer-count`, `.status-capsule-disconnected`, and `.peer-pill`.<br>2. Remove background and glow from `.btn-message-burn`.<br>3. Remove / clean up `.system-timestamp` and `.message-timestamp` styles. |
| `zero_client/index.html` | 1. Update welcome splash markup and `WELCOME_SPLASH_HTML` to `"Create -> Share -> Chat"`.<br>2. Implement unified UI state updates according to Rules 1–4.<br>3. Omit timestamp rendering in `appendSystemMessage` and `appendMessageBubble`.<br>4. Fix `isJoining` lifecycle so `"Joined chat"` is emitted only on join and never duplicated with `"Peer joined chat"`.<br>5. Immediately close engine and restore UI on connection failure. |
| `zero_client/src/ui.rs` | Remove timestamp span rendering in `create_message_element` to keep Rust WASM UI client in sync with `index.html`. |
| `zero_client/tests/browser/test_mesh_scenario.js` | 1. Update existing assertions that expected timestamps in system messages to assert absence.<br>2. Add Test 17 covering all UI rules, badge minimalism, splash text, clean burn icon, join message integrity, and immediate failure recovery. |
| `openspec/specs/overview.md` | Reconcile canonical spec with UI state rules, splash text, and update test metrics. |

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
   - `bash zero_client/tests/browser/run_test.sh` (17 / 17 tests pass, including updated assertions and new Test 17).
5. **Gate 5: Architectural & Security Review**: Reviewer Cycle 1 verification and unanimous `APPROVE` verdict.
