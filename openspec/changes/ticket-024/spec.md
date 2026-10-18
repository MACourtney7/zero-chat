# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-024"
title: "Peer Departure Deduplication and Disconnected Room Controls Preservation"
state: "IN_PROGRESS"
created_at: "2026-10-18T20:25:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-024/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & Objective
When a peer exits a chat session—whether cleanly via `#header-leave-btn`, abruptly via browser refresh/navigation/close, or through network severance ("dirty disconnect")—the surviving participant experiences two jarring UX bugs:

1. **Duplicated & Inconsistent System Messages**:
   - Currently, an abrupt exit or page refresh fires multiple events in rapid succession (`beforeunload`, `pagehide`, and WebRTC DataChannel/PeerConnection closure).
   - `onMessageCb` intercepts incoming `leave_and_burn` frames and logs `[Peer left - messages burned]` (often duplicated if both unload events transmit).
   - Subsequently, `onStateChangeCb` detects `st === "closed"` and appends `[Peer disconnected - messages burned]`.
   - The user sees up to 3 conflicting/duplicate system messages for a single departure event.
   - **Objective**: Regardless of how a peer leaves (graceful leave button, page refresh, tab closure, or dirty disconnect), **exactly one** clean system message must be logged: `"Peer left chat"`.

2. **Premature Collapse of Room Controls & Stranded Chat State**:
   - When `st === "closed"` fires, the UI currently invokes a partial reset: it hides `#header-leave-btn`, hides `#burn-room-bar`, empties and hides `#peer-list` (which holds the lock icon), unhides the idle status blue dot `#idle-status-capsule`, hides `#message-form`, and shows `#create-room-bar` (the green new-chat button) at the bottom.
   - However, the message transcript and user's own outgoing messages remain on screen.
   - Because the leave button and burn button have disappeared, the user is stranded in a crippled room state where they cannot cleanly leave or burn the chat to wipe history and return to their "home" view.
   - **Objective**: When a peer leaves/disconnects during an active chat, the room UI must preserve active session controls in a disconnected state:
     - Top bar displays: `0` peers counter, padlock icon/pill, and leave button. The idle status blue dot remains hidden.
     - Sub-header displays: `#burn-room-bar` (burn button) remains visible and accessible.
     - Footer input: `#message-form` remains displayed with inputs disabled (`msgInput.disabled = true`, `sendBtn.disabled = true`, placeholder updated to indicate peer departure). `#create-room-bar` remains hidden.
     - Leaving or Burning: When the surviving user clicks either `#header-leave-btn` or `#destroy-session-button`, `resetToHome()` executes cleanly, wiping all messages, restoring the welcome splash, restoring `#create-room-bar`, and returning the header to the idle state.

---

## 2. Scope Boundaries

- **In-Scope**:
  - `zero_client/index.html`:
    - **Departure Deduplication Latch**: Introduce an idempotent departure handler / state latch (`hasPeerLeft` / session tracking) that ensures a departing peer triggers the departure lifecycle and logs `"Peer left chat"` at most once per session.
    - **Single Standardized System Message**: Replace all bracketed messages (`[Peer left - messages burned]`, `[Peer disconnected - messages burned]`) with a single clean system notice: `"Peer left chat"`.
    - **Disconnected Room UI State**:
      - When a peer leaves or connection closes during an active chat:
        - `peerCountEl`: set to `"0"`
        - `peerListEl` (padlock pill): kept visible with lock icon intact
        - `headerLeaveBtn`: kept visible
        - `idleStatusCapsule`: remains hidden
        - `burnRoomBar`: remains visible below header
        - `messageForm`: remains visible with `msgInput.disabled = true` and `sendBtn.disabled = true`
        - `createRoomBar`: remains hidden
      - In `resetToHome()`: ensure full restoration of home idle state (feed cleared, splash shown, `createRoomBar` shown, `headerLeaveBtn` hidden, `burnRoomBar` hidden, padlock hidden, `idleStatusCapsule` visible).
    - **Opportunistic Dissemination Deduplication**: Guard `sendOpportunisticLeave` against sending duplicate frames across `beforeunload` and `pagehide`.
  - `zero_client/src/ui.rs`:
    - Ensure native test double and DOM bindings reflect the disconnected room state requirements: peer counter displaying 0, padlock present, leave and burn actions available, transition back to home state upon `destroy_session`.
  - `zero_client/tests/mesh_simulation_test.rs`:
    - Verify that dirty disconnect and leave event triggers continue to operate seamlessly across the mesh.

- **Out-of-Scope**:
  - Modifying backend Durable Object or Cloudflare Worker signaling protocols.
  - Adding persistent message storage or offline messaging.

---

## 3. Requirements & Acceptance Criteria

- [ ] **REQ-024.1 (Deduplicated Peer Departure System Message)**:
  - Regardless of how a peer leaves (clicking `#header-leave-btn`, page refresh, tab closure, or network disconnect), surviving clients must produce **exactly one** system message: `"Peer left chat"`.
  - No duplicate system messages may appear on page refresh or dirty disconnect.
  - The bracketed messages `[Peer left - messages burned]` and `[Peer disconnected - messages burned]` must be eliminated in favor of `"Peer left chat"`.

- [ ] **REQ-024.2 (Selective Message Purge Preservation)**:
  - When the peer leaves (either via signal or dirty disconnect), remote messages authored by that peer are immediately removed from `#messages-feed`, preserving the zero-persistence invariant.

- [ ] **REQ-024.3 (Disconnected Room Top Bar & Burn Controls)**:
  - When a peer leaves/disconnects in an active session:
    1. Header peer counter `#peer-count` displays `"0"`.
    2. Header padlock icon (`#peer-list .peer-pill`) remains visible.
    3. Header leave button `#header-leave-btn` remains visible.
    4. Header idle status capsule `#idle-status-capsule` remains hidden.
    5. Sub-header burn button bar `#burn-room-bar` remains visible below the header.
    6. Footer `#message-form` remains visible with inputs disabled; `#create-room-bar` remains hidden.

- [ ] **REQ-024.4 (Clean Reset to Home via Leave or Burn)**:
  - From the disconnected room state, clicking either `#header-leave-btn` or `#destroy-session-button` must trigger `resetToHome()`:
    1. Message feed is wiped (`#messages-container` returns to welcome splash).
    2. Header controls transition to idle state (padlock hidden, leave button hidden, `#idle-status-capsule` visible with blue dot, peer counter `"0"`).
    3. `#burn-room-bar` is hidden.
    4. Footer `#create-room-bar` is unhidden and `#message-form` is hidden.
    5. URL hash is cleared.

- [ ] **REQ-024.5 (Automated Test Coverage & Linter Integrity)**:
  - Automated tests verify:
    - Peer departure deduplication and single `"Peer left chat"` output.
    - Top bar and burn button retention on peer departure.
    - Clean return to home view upon leave/burn.
  - Full workspace test suite (`cargo test --workspace`) passes with 100% success.
  - Zero compiler or clippy warnings (`cargo clippy --workspace`).

---

## 4. Architectural Approach & Implementation Plan

### Client-Side State Transition Architecture
```
[Active Chat Session: 1 Peer]
  |
  +---> Peer leaves (leave_and_burn / beforeunload / pagehide / st === "closed")
  |
  v
[Disconnected Room State: 0 Peers]
  - Top Bar: [ 0 ] [Padlock] [Leave Button]
  - Sub Bar: [Burn Chat Button]
  - Feed: User outgoing messages + single system message "Peer left chat" (remote peer messages purged)
  - Footer: Disabled chat input ("Peer left chat")
  |
  +---> User clicks [Leave Button] OR [Burn Chat Button]
  |
  v
[Home Idle View]
  - Top Bar: [ 0 ] [Blue Dot]
  - Sub Bar: (Hidden)
  - Feed: Welcome Splash
  - Footer: [Create Private Chat Button]
```

### Planned Brief Decomposition (`implement-task`)
1. **Brief 1 (`brief-impl-client-leave-ui.md`) [MEDIUM]**:
   - Update `zero_client/index.html`:
     - Implement departure latch to suppress duplicate frames / disconnects.
     - Emit single `"Peer left chat"` system message.
     - Preserve top bar controls (`peerCountEl = "0"`, padlock visible, leave button visible, idle capsule hidden) and `#burn-room-bar` visible on peer departure.
     - Keep message form visible in disabled state; leave `#create-room-bar` hidden until leave/burn is clicked.
     - Ensure `resetToHome()` cleanly returns to home view.
2. **Brief 2 (`brief-val-tests.md`) [MEDIUM]**:
   - Update `zero_client/src/ui.rs` and `zero_client/tests/mesh_simulation_test.rs` to validate the deduplicated `"Peer left chat"` lifecycle, top bar state preservation, and clean return to home view.
