# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-039"
title: "UI Tweaks: Add Peer Icon, Bottom-Left Invite Button, Top-Bar Home Cleanliness, Zero-Peer Slashed Badge & Blue Dot Unification"
state: "COMPLETED"
created_at: "2026-10-25T08:45:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-039/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

Through visual testing and iterative design review, five user interface refinements have been requested to streamline navigation and unify top-bar / footer interactions:

1. **Add Peer Icon (`+[person_icon]`)**:
   - The Stage 1 Plus bottom button variant previously displayed a generic plus sign (`+`).
   - **Intent**: Replace the bare `+` icon with a dedicated "add peers" icon featuring a person silhouette with a plus sign (`+[person_icon]`, e.g. `<svg viewBox="0 0 24 24"><path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"></path><circle cx="9" cy="7" r="4"></circle><line x1="19" y1="8" x2="19" y2="14"></line><line x1="22" y1="11" x2="16" y2="11"></line></svg>`).
2. **In-Room Add Peer Button Left of Text Input (Precursor to Nested Attachments)**:
   - In active chat (`inRoom && openPeerCount > 0`), users previously had to reach for the small `+` icon in the top-right header to invite additional peers.
   - Floating a full-width bar above the input adds vertical bulk and visual competition with the top burn bar, while dynamic swap risks trapped drafts.
   - **Intent**: Remove `#header-add-btn` from the top bar entirely. Place an inlined Add Peer button (`#input-add-peer-btn`) on the **left side of the text input** inside `#message-form`, displaying the `+[person_icon]` SVG.
   - This keeps the input bar as a compact single row (~54px total height), prevents thumb collision with Send, preserves draft text safety, and establishes the bottom-left action anchor for a future nested attachment flow (where Add User and file attachments can be combined into an action drawer).
3. **Clean Home View Top Bar**:
   - On the Home view (`!inRoom`), the top bar previously showed the slashed person icon badge and the blue dot capsule.
   - **Intent**: Remove badges from the top bar at home view (`#peer-count` `[no_peers_badge]`, `#idle-status-capsule` `[blue_dot_badge]`, and `#peer-list` are all hidden). The top bar displays only the brand text on the left, with zero badges or buttons on the right.
4. **Zero-Peer Slashed Badge Invariant**:
   - When a session is entered or all peers depart (`inRoom && openPeerCount === 0`), the peer counter previously rendered the numeric character `"0"`.
   - **Intent**: Replace numeric `"0"` with `[no_peers_badge]` (`SLASHED_PERSON_ICON_SVG`). Inside a room session with 0 peers, `#peer-count` renders the slashed person icon. When peers > 0, `#peer-count` renders the numeric string (`"1"`, `"2"`...).
5. **Zero-Peer Badge Unification (Blue Dot Instead of Padlock)**:
   - Previously, when a user started a new chat (host waiting), the header showed `[0_peer_counter] [blue dot] [leave]`. However, when peers departed (limbo state), the header showed `[0_peer_counter] [padlock] [leave]`.
   - **Intent**: Unify the UI state across all 0-peer room states: whenever the 0-peer counter badge is displayed, the blue dot badge (`#idle-status-capsule`) MUST be displayed, and the padlock (`#peer-list`) MUST be hidden. The padlock is reserved strictly for active encrypted sessions with >0 peers. Both host waiting and peer limbo show identical top-bar icons: `[no_peers_badge] [blue dot] [leave]`.

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
  - Both `#btn-create-room` (in 0-peer room view) and `#input-add-peer-btn` (in active chat) use `ADD_PEER_ICON_SVG` and `aria-label="Add peer"`.

### REQ-039.2: Top-Bar Cleanliness at Home View
- In `zero_client/index.html` `updatePeerCountDisplay()`:
  - When `!inRoom` (Home view):
    - `#peer-count` (`peerCountEl`) is **hidden** (`classList.add("hidden")`).
    - `#idle-status-capsule` (`idleStatusCapsule`) is **hidden** (`classList.add("hidden")`).
    - `#peer-list` (`peerListEl`) is **hidden** (`classList.add("hidden")` and `innerHTML = ""`).
    - `#header-leave-btn` (`headerLeaveBtn`) is **hidden** (`classList.add("hidden")`).
    - `#header-add-btn` (`headerAddBtn`) is **hidden** (`classList.add("hidden")`).
    - `#message-form` is **hidden**.
    - `#create-room-bar` is **visible** in Create Chat mode (`setCreateRoomStage1()`).

### REQ-039.3: Zero-Peer Slashed Badge & Numeric Counter Lifecycle (Blue Dot Unification)
- In `zero_client/index.html` `updatePeerCountDisplay()` and limbo disconnect handler:
  - When `inRoom && openPeerCount === 0` (host waiting, limbo disconnect, join failure):
    - `#peer-count` is **visible** (`classList.remove("hidden")`) and renders `SLASHED_PERSON_ICON_SVG` (`[no_peers_badge]`) instead of `"0"`.
    - `#idle-status-capsule` (blue dot) is **visible** (`classList.remove("hidden")`).
    - `#peer-list` (padlock) is **hidden** (`classList.add("hidden")` and `innerHTML = ""`).
    - `#header-leave-btn` is **visible** (`classList.remove("hidden")`).
    - `#message-form` is **hidden**.
    - `#create-room-bar` is **visible** (`classList.remove("hidden")`):
      - **Host Waiting (Initial creation from Home view)**: Since the invite URL was generated and copied to the clipboard during room creation, `#btn-create-room` is in **Stage 2 (`.btn-waiting`, Copy Icon)**. Clicking it re-copies the invite link to the clipboard and restarts temporary feedback notices.
      - **Limbo Disconnect (Peers have left)**: Before a new invite is requested, `#btn-create-room` is initialized in **Stage 1 Plus (`+[person_icon]`)**. Clicking it generates a new in-room offer, copies to clipboard, and transitions to **Stage 2 (`.btn-waiting`, Copy Icon)**.
      - `updatePeerCountDisplay()` preserves `.btn-waiting` if already in Stage 2 (never prematurely resets active Stage 2 copy buttons).
    - **UI Unification Invariant**: Whenever the 0-peer counter badge is displayed, the blue dot badge (`#idle-status-capsule`) is always displayed alongside it (`[no_peers_badge] [blue dot] [leave]`), and the padlock icon (`#peer-list`) is strictly hidden. Both host waiting and peer departure limbo states present identical top-bar controls.
  - When `inRoom && openPeerCount > 0` (active chat with 1+ peers):
    - `#peer-count` is **visible** (`classList.remove("hidden")`) and renders numeric string `String(count)` (`"1"`, `"2"`...).
    - `#idle-status-capsule` is **hidden** (`classList.add("hidden")`).
    - `#peer-list` (padlock) is **visible** (`classList.remove("hidden")`).
    - `#header-leave-btn` is **visible** (`classList.remove("hidden")`).
    - `#create-room-bar` is **hidden** (`classList.add("hidden")`).
    - `#message-form` is **visible** (`classList.remove("hidden")`).

### REQ-039.4: Remove Top Bar `+` Icon & Place `+[person_icon]` Left of Text Input
- In `zero_client/index.html`:
  - Permanently remove or hide `#header-add-btn` from `#header-controls`.
  - In `#message-form`, add `#input-add-peer-btn` immediately before `#message-input`:
    ```html
    <form id="message-form" class="message-form hidden" onsubmit="return false;">
        <button id="input-add-peer-btn" class="btn btn-action-peer btn-icon" type="button" aria-label="Add peer">
            <!-- ADD_PEER_ICON_SVG -->
        </button>
        <input id="message-input" class="message-input" type="text" autocomplete="off" disabled />
        <button id="send-button" class="btn btn-send-emerald btn-icon" type="button" aria-label="Send message" disabled>
            ...
        </button>
    </form>
    ```
  - Clicking `#input-add-peer-btn` in active chat:
    - Generates in-room peer offer and dead-drop signaling socket (`pendingInviteWs`).
    - Copies invite URL to clipboard.
    - Transitions `#input-add-peer-btn` to Stage 2 (`.btn-waiting`, copy icon).
    - Emits immediate Notice 1 (`"Invite link copied to clipboard"`) and staggered Notice 2 (`"Waiting for peer..."`).
    - Re-clicks re-copy the URL and restart notices without disabling or clearing the message text input.
    - Preserves all drafted text in `#message-input` with zero loss or friction.
- In `zero_client/style.css`:
  - Style `#input-add-peer-btn` / `.btn-action-peer`:
    - Dimensions: `width: 44px; height: 42px;` (symmetrical with `#send-button`).
    - Background: `rgba(56, 189, 248, 0.12); border: 1px solid rgba(56, 189, 248, 0.3); color: var(--accent-cyan); border-radius: 8px;`.
    - Hover: `background: rgba(56, 189, 248, 0.22); border-color: rgba(56, 189, 248, 0.5); transform: translateY(-1px);`.
    - Active: `transform: translateY(0);`.
    - Waiting state (`.btn-waiting`): `background: rgba(245, 158, 11, 0.15); border-color: rgba(245, 158, 11, 0.4); color: #fbbf24;`.

### REQ-039.5: Automated Regression Test Suite Updates
- In `zero_client/tests/browser/test_mesh_scenario.js`:
  - Update Home view assertions:
    - Assert `#peer-count` is hidden (`classList.contains("hidden") === true`).
    - Assert `#idle-status-capsule` is hidden (`classList.contains("hidden") === true`).
    - Assert `#peer-list` is hidden (`classList.contains("hidden") === true`).
  - Update 0-peers room assertions (both host waiting and peer limbo after disconnect):
    - Assert `#peer-count` is visible and contains `peer-slashed-user-icon` SVG (instead of text `"0"`).
    - Assert `#idle-status-capsule` is visible (`classList.contains("hidden") === false`).
    - Assert `#peer-list` is hidden (`classList.contains("hidden") === true`).
    - Assert `#header-leave-btn` is visible.
  - Update active chat assertions:
    - Assert `#header-add-btn` is hidden or omitted from top bar.
    - Assert `#input-add-peer-btn` is visible on the left of `#message-input` inside `#message-form`.
    - Assert `#input-add-peer-btn` contains `+[person_icon]` icon.
    - Assert `#peer-list` is visible and contains padlock icon.
    - Assert clicking `#input-add-peer-btn` in active chat copies invite link, morphs to copy icon, and emits notices cleanly without altering draft input text.
  - All 17 browser headless tests must pass (`bash zero_client/tests/browser/run_test.sh`).
  - All 123 Rust workspace tests must pass (`cargo test --workspace`).

---

## 3. Target Architecture & Affected Modules

| Module / File | Changes & Responsibilities |
|---|---|
| `zero_client/index.html` | 1. Update `ADD_PEER_ICON_SVG` to user-plus vector icon.<br>2. In Home view: hide `#peer-count`, `#idle-status-capsule`, and `#peer-list`.<br>3. In 0-peer room view & limbo state: render `SLASHED_PERSON_ICON_SVG` in `#peer-count`, show `#idle-status-capsule` (blue dot), hide `#peer-list` (padlock).<br>4. In active chat: remove `#header-add-btn`, add `#input-add-peer-btn` immediately before `#message-input` with `ADD_PEER_ICON_SVG`, wire invite flow. |
| `zero_client/style.css` | Style `#input-add-peer-btn` / `.btn-action-peer` with cyan accent, 44px × 42px dimensions symmetrical with `#send-button`, hover and `.btn-waiting` states. |
| `zero_client/tests/browser/test_mesh_scenario.js` | Update assertions across test scenarios for hidden home badges, slashed badge for 0 peers, unified blue dot (no padlock) in 0-peer states, and bottom-left `#input-add-peer-btn` in active chat. |
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
