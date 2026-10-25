# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-040"
title: "In-Chat Add Peer Button: 2-Stage Blue/Cyan Styling and 5s Temporary Notice Timeout"
state: "PLANNING"
created_at: "2026-10-25T10:38:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-040/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

Through visual testing and iterative UX review of the newly introduced in-chat bottom-left Add Peer button (`#input-add-peer-btn`), two behavioral refinements have been identified:

1. **Eliminate Orange Coloring in Chat View (2 Stages of Blue/Cyan)**:
   - When clicked, `#input-add-peer-btn` currently adopts `.btn-waiting`, shifting its background and icon to amber/orange (`#fbbf24`, `rgba(245, 158, 11, 0.15)`).
   - This clashes visually with the cyan/emerald palette of the input bar and message feed.
   - **Intent**: The in-chat `#input-add-peer-btn` must strictly use 2 stages of blue/cyan styling:
     - **Stage 1 (Default / Idle)**: Subtle translucent cyan (`background: rgba(56, 189, 248, 0.12); border-color: rgba(56, 189, 248, 0.3); color: var(--accent-cyan);`) displaying `ADD_PEER_ICON_SVG` (`+[person_icon]`).
     - **Stage 2 (Active / Copied)**: Deeper/accented blue/cyan (`background: rgba(56, 189, 248, 0.28); border-color: rgba(56, 189, 248, 0.65); color: #38bdf8;`) displaying `COPY_ICON_SVG` (`[copy_icon]`). Completely eliminate orange/amber in the active chat view.
2. **Restore 5-Second Notice Timeout & Eliminate "Waiting for peer..." in Chat View**:
   - In active chat view (`openPeerCount > 0`), clicking `#input-add-peer-btn` currently invokes `triggerCreateRoomNotices()`, which emits permanent/semi-permanent notices: `"Invite link copied to clipboard"` followed by a staggered `"Waiting for peer..."` message with ticking dots.
   - In an active chat where participants are already connected and conversing, `"Waiting for peer..."` is inaccurate, distracting, and clutters the conversation feed.
   - **Intent**: Restore the previous top-bar button invite behavior (`triggerInChatInviteFeedback()`):
     - Emit a single clean notice: `"Invite link copied to clipboard"`.
     - Apply `.notice-pulse` for subtle animation.
     - Auto-dismiss and remove the notice from the DOM after exactly 5 seconds (`setTimeout(..., 5000)`).
     - Never emit `"Waiting for peer..."` in active chat view.
     - Re-clicks re-copy the invite URL, re-pulse the notice, and reset the 5-second timer.
     - Synchronize the button state: after the 5-second timeout expires, `#input-add-peer-btn` cleanly reverts from Stage 2 (copy icon) back to Stage 1 (`+[person_icon]`), while caching the pending invite for instant re-copying.

---

## 2. Requirements & Acceptance Criteria

### REQ-040.1: In-Chat Add Peer Button 2-Stage Blue/Cyan Styling
- In `zero_client/style.css`:
  - Scoped specifically to `.btn-action-peer` (or `.btn-action-peer.btn-waiting`):
    - Completely eliminate amber/orange colors (`#fbbf24`, `rgba(245, 158, 11, ...)`).
    - Define Stage 2 styling using the blue/cyan palette:
      ```css
      .btn-action-peer.btn-waiting {
          background: rgba(56, 189, 248, 0.28);
          border-color: rgba(56, 189, 248, 0.65);
          color: #38bdf8;
      }
      ```
    - The full-width `#btn-create-room` in 0-peer room view / limbo retains its independent styling.

### REQ-040.2: 5-Second Notice Timeout & Elimination of "Waiting for peer..." in Active Chat
- In `zero_client/index.html`:
  - Implement a dedicated in-chat feedback handler `triggerInChatInviteFeedback()`:
    ```javascript
    let inChatInviteFeedbackTimer = null;
    let inChatInviteFeedbackEl = null;

    function triggerInChatInviteFeedback() {
        if (inChatInviteFeedbackTimer) {
            clearTimeout(inChatInviteFeedbackTimer);
            inChatInviteFeedbackTimer = null;
        }

        if (inChatInviteFeedbackEl && inChatInviteFeedbackEl.parentNode === feed) {
            inChatInviteFeedbackEl.classList.remove("notice-pulse");
            void inChatInviteFeedbackEl.offsetWidth;
            inChatInviteFeedbackEl.classList.add("notice-pulse");
        } else {
            clearTransientStatus();
            inChatInviteFeedbackEl = document.createElement("div");
            inChatInviteFeedbackEl.className = "system-message system-temporary in-chat-invite-notice notice-pulse";

            const textSpan = document.createElement("span");
            textSpan.className = "system-text";
            textSpan.textContent = "Invite link copied to clipboard";
            inChatInviteFeedbackEl.appendChild(textSpan);

            feed.appendChild(inChatInviteFeedbackEl);
            feed.scrollTop = feed.scrollHeight;
        }

        inChatInviteFeedbackTimer = setTimeout(() => {
            if (inChatInviteFeedbackEl && inChatInviteFeedbackEl.parentNode) {
                inChatInviteFeedbackEl.remove();
            }
            inChatInviteFeedbackEl = null;
            inChatInviteFeedbackTimer = null;
            // Revert button to Stage 1 after timeout
            resetInputAddPeerButton();
        }, 5000);
    }
    ```
  - Clicking `#input-add-peer-btn` calls `triggerInChatInviteFeedback()` instead of `triggerCreateRoomNotices()`.
  - In active chat view, `"Waiting for peer..."` is **never** appended to the feed.

### REQ-040.3: Button State Reversion Synchronized with 5s Notice
- In `zero_client/index.html`:
  - When `#input-add-peer-btn` is clicked in active chat:
    - Transitions to Stage 2 (`.btn-waiting`, `COPY_ICON_SVG`, `aria-label="Copy invite link"`).
  - When the 5-second timer expires:
    - Reverts `#input-add-peer-btn` to Stage 1 (`ADD_PEER_ICON_SVG`, `aria-label="Add peer"`, removes `.btn-waiting`).
  - Re-clicking within the 5-second window:
    - Re-copies the invite URL to clipboard.
    - Re-animates `notice-pulse` on the existing notice.
    - Resets the 5-second timeout, keeping the button in Stage 2 and notice visible for another 5 seconds.
  - When a peer joins or when leaving the room:
    - Any active `inChatInviteFeedbackTimer` is cleared, `inChatInviteFeedbackEl` is removed, and `#input-add-peer-btn` is restored to Stage 1.
  - Message input text draft in `#message-input` is never modified or cleared.

### REQ-040.4: Automated Browser Regression Test Updates
- In `zero_client/tests/browser/test_mesh_scenario.js`:
  - Update Test 13 and related active chat assertions:
    - Assert `#input-add-peer-btn` does NOT contain orange/amber styles upon click.
    - Assert exactly 1 temporary notice `"Invite link copied to clipboard"` is appended to the feed.
    - Assert `"Waiting for peer..."` is NOT in the feed.
    - Assert advancing fake timers by 5000ms removes the temporary notice from the feed.
    - Assert `#input-add-peer-btn` reverts to Stage 1 (`ADD_PEER_ICON_SVG`) after 5000ms.
    - Assert re-clicking within 5000ms re-copies link, restarts 5000ms timer, and preserves notice.
  - All 17 browser headless tests must pass (`bash zero_client/tests/browser/run_test.sh`).
  - All 123 Rust workspace tests must pass (`cargo test --workspace`).

---

## 3. Target Architecture & Affected Modules

| Module / File | Changes & Responsibilities |
|---|---|
| `zero_client/index.html` | Implement `triggerInChatInviteFeedback()` with 5-second auto-dismiss timeout, wire `#input-add-peer-btn` to it, and handle 5s Stage 2 -> Stage 1 button reversion without emitting `"Waiting for peer..."`. |
| `zero_client/style.css` | Update `.btn-action-peer.btn-waiting` to use Stage 2 blue/cyan styling (`rgba(56, 189, 248, 0.28)`, `#38bdf8`), eliminating amber/orange. |
| `zero_client/tests/browser/test_mesh_scenario.js` | Update Test 13 assertions to verify the 5s notice timeout, absence of `"Waiting for peer..."`, and 2-stage blue/cyan button behavior in chat view. |
| `openspec/specs/overview.md` | Reconcile canonical spec Invariant 16 upon task completion. |

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
