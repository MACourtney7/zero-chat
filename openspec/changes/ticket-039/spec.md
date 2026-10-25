# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-039"
title: "UI Tweaks: Add Peer Icon, In-Room Invite Bar Above Input, Top-Bar Home Cleanliness & Zero-Peer Slashed Badge"
state: "PLANNING"
created_at: "2026-10-25T08:45:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-039/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

Through visual testing and iterative design review, four user interface refinements have been requested to streamline navigation and simplify top-bar / footer interactions:

1. **Add Peer Icon (`+[person_icon]`)**:
   - The Stage 1 Plus bottom button variant previously displayed a generic plus sign (`+`).
   - **Intent**: Replace the bare `+` icon with a dedicated "add peers" icon featuring a person silhouette with a plus sign (`+[person_icon]`, e.g. `<svg viewBox="0 0 24 24"><path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"></path><circle cx="9" cy="7" r="4"></circle><line x1="19" y1="8" x2="19" y2="14"></line><line x1="22" y1="11" x2="16" y2="11"></line></svg>`).
2. **Persistent In-Room Invite Bar Above Text Input**:
   - In active chat (`inRoom && openPeerCount > 0`), the bottom button was previously hidden, requiring users to look for and click the small `+` button in the top bar to invite additional peers.
   - **Intent**: Remove the `+` button from the top bar entirely (`#header-add-btn`). Instead, display the bottom button (`#create-room-bar`) in the `+[person_icon]` variant directly above the text input (`#message-form`). Both the invite button and message input are visible simultaneously in active chat. Clicking the invite button copies the invite link, transitions to Stage 2 (`.btn-waiting`, copy icon), and emits feedback notices without interfering with the text input.
3. **Clean Home View Top Bar**:
   - On the Home view (`!inRoom`), the top bar previously showed the slashed person icon badge and the blue dot capsule.
   - **Intent**: Remove badges from the top bar at home view (`#peer-count` `[no_peers_badge]` and `#idle-status-capsule` `[blue_dot_badge]` are both hidden). The top bar displays only the brand text on the left, with zero badges or buttons on the right.
4. **Zero-Peer Slashed Badge Invariant**:
   - When a session is entered or all peers depart (`inRoom && openPeerCount === 0`), the peer counter previously rendered the numeric character `"0"`.
   - **Intent**: Replace numeric `"0"` with `[no_peers_badge]` (`SLASHED_PERSON_ICON_SVG`). Inside a room session with 0 peers, `#peer-count` renders the slashed person icon, the blue dot capsule is visible, and the Leave button is visible. When peers > 0, `#peer-count` renders the numeric string (`"1"`, `"2"`...), the blue dot is hidden, and the Leave button remains visible.

---

## 2. Requirements & Acceptance Criteria

### REQ-039.1: Add Peer Icon (`+[person_icon]`)
- In `zero_client/index.html`:
  - Update `ADD_PEER_ICON_SVG` from a bare plus sign to a user-plus icon featuring a person silhouette and plus symbol:
    ```javascript
    const ADD_PEER_ICON_SVG = `
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"></path>
            <circle cx="9" cy="7" r="4"></circle>
            <line x1="19" y1="8" x2="19" y2="14"></line>
            <line x1="22" y1="11" x2="16" y2="11"></line>
        </svg>`.trim();
    ```
  - `setCreateRoomStage1Plus()` sets `btnCreateRoom.innerHTML = ADD_PEER_ICON_SVG;` and `aria-label="Add peer"`.

### REQ-039.2: Top-Bar Cleanliness at Home View
- In `zero_client/index.html` `updatePeerCountDisplay()`:
  - When `!inRoom` (Home view):
    - `#peer-count` (`peerCountEl`) is **hidden** (`classList.add("hidden")`).
    - `#idle-status-capsule` (`idleStatusCapsule`) is **hidden** (`classList.add("hidden")`).
    - `#header-leave-btn` (`headerLeaveBtn`) is **hidden** (`classList.add("hidden")`).
    - `#header-add-btn` (`headerAddBtn`) is **hidden** (`classList.add("hidden")`).
    - `#message-form` is **hidden**.
    - `#create-room-bar` is **visible** in Create Chat mode (`setCreateRoomStage1()`).

### REQ-039.3: Zero-Peer Slashed Badge & Numeric Counter Lifecycle
- In `zero_client/index.html` `updatePeerCountDisplay()`:
  - When `inRoom && openPeerCount === 0` (host waiting, limbo disconnect, join failure):
    - `#peer-count` is **visible** (`classList.remove("hidden")`) and renders `SLASHED_PERSON_ICON_SVG` (`[no_peers_badge]`) instead of `"0"`.
    - `#idle-status-capsule` is **visible** (`classList.remove("hidden")`).
    - `#header-leave-btn` is **visible** (`classList.remove("hidden")`).
    - `#message-form` is **hidden**.
    - `#create-room-bar` is **visible** in Stage 1 Plus mode (`setCreateRoomStage1Plus()`, `+[person_icon]`).
  - When `inRoom && openPeerCount > 0` (active chat with 1+ peers):
    - `#peer-count` is **visible** (`classList.remove("hidden")`) and renders numeric string `String(count)` (`"1"`, `"2"`...).
    - `#idle-status-capsule` is **hidden** (`classList.add("hidden")`).
    - `#header-leave-btn` is **visible** (`classList.remove("hidden")`).

### REQ-039.4: Remove Top Bar `+` Icon & Display `+[person_icon]` Above Text Input
- In `zero_client/index.html`:
  - Permanently remove or hide `#header-add-btn` from `#header-controls`.
  - In active chat (`inRoom && openPeerCount > 0`):
    - `#create-room-bar` is **visible** (`classList.remove("hidden")`) above `#message-form`.
    - `#message-form` is **visible** (`classList.remove("hidden")`) with input enabled.
    - If `btnCreateRoom` is not in waiting state, render in Stage 1 Plus mode (`setCreateRoomStage1Plus()`).
  - Clicking `#btn-create-room` in active chat:
    - Generates in-room peer offer and dead-drop signaling socket (`pendingInviteWs`).
    - Copies invite URL to clipboard.
    - Transitions `#btn-create-room` to Stage 2 (`.btn-waiting`, copy icon).
    - Emits immediate Notice 1 (`"Invite link copied to clipboard"`) and staggered Notice 2 (`"Waiting for peer..."`).
    - Re-clicks re-copy the URL and restart notices without disabling or clearing the message text input.
- In `zero_client/style.css`:
  - Ensure `.chat-footer` uses flex column layout with consistent gap (`gap: 8px;`) when both `#create-room-bar` and `#message-form` are visible.

### REQ-039.5: Automated Regression Test Suite Updates
- In `zero_client/tests/browser/test_mesh_scenario.js`:
  - Update Home view assertions across Test 14 and Test 17:
    - Assert `#peer-count` is hidden (`classList.contains("hidden") === true`).
    - Assert `#idle-status-capsule` is hidden (`classList.contains("hidden") === true`).
  - Update 0-peers room assertions:
    - Assert `#peer-count` is visible and contains `peer-slashed-user-icon` SVG (instead of text `"0"`).
  - Update active chat assertions:
    - Assert `#header-add-btn` is hidden or omitted from top bar.
    - Assert `#create-room-bar` is visible above `#message-form`.
    - Assert `#btn-create-room` contains `+[person_icon]` icon.
    - Assert clicking `#btn-create-room` in active chat copies invite link, morphs to copy icon, and emits notices cleanly.
  - All 17 browser headless tests must pass (`bash zero_client/tests/browser/run_test.sh`).
  - All 123 Rust workspace tests must pass (`cargo test --workspace`).

---

## 3. Target Architecture & Affected Modules

| Module / File | Changes & Responsibilities |
|---|---|
| `zero_client/index.html` | 1. Update `ADD_PEER_ICON_SVG` to user-plus vector icon.<br>2. In Home view: hide `#peer-count` and `#idle-status-capsule`.<br>3. In 0-peer room view: render `SLASHED_PERSON_ICON_SVG` in `#peer-count`.<br>4. In active chat: remove `#header-add-btn`, display `#create-room-bar` above `#message-form`. |
| `zero_client/style.css` | Ensure `.chat-footer` flex column handles stacking of `#create-room-bar` and `#message-form` cleanly with `gap: 8px;`. |
| `zero_client/tests/browser/test_mesh_scenario.js` | Update assertions across test scenarios for hidden home badges, slashed badge for 0 peers, and footer invite bar in active chat. |
| `openspec/specs/overview.md` | Reconcile canonical spec with Invariant 16 upon task completion. |

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
   - `bash zero_client/tests/browser/run_test.sh` (17 / 17 tests pass).
5. **Gate 5: Architectural & Security Review**: Reviewer verification with `APPROVE` verdict.
