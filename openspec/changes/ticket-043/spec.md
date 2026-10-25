# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-043"
title: "UI Architecture Fixes: Glow Clipping, Burn/Leave Theme Alignment, Limbo Burn Visibility, Mobile Bubble Flex Containment, and Ellipsis De-duplication"
state: "IN_PROGRESS"
created_at: "2026-10-25T16:15:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-043/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

Following the visual modernization in `ticket-042`, five specific UI glitches and behavioral discrepancies were reported on desktop and mobile viewports:
1. **Bottom Button On-Hover Glow Clipping**: When hovering `#send-button` or `#input-add-peer-btn`, a dark patch/cutoff appears along the top perimeter of the circular button glow. This is caused by `overflow-x: hidden` and zero top padding on `.message-form`, which forces browsers to clip vertical `box-shadow` extending beyond `y = 0`.
2. **Burn Button Theme Mismatch with Leave Button**: `#destroy-session-button` in the header currently uses red-500 palette (`rgba(239, 68, 68, ...)`), a 28px height, and `var(--accent-rose)` text, while the adjacent `#header-leave-btn` uses a cooler rose-500 palette (`rgba(244, 63, 94, ...)`), a 26px height, and `#fda4af` text. The two header action buttons appear visually mismatched when rendered in the same row.
3. **Burn Button Hidden in Limbo State**: In `ticket-042`, `#destroy-session-button` was hidden when `openPeerCount === 0`. However, the user requires that whenever `#header-leave-btn` is visible (any in-room session), `#destroy-session-button` must also remain visible, enabling users to incinerate a room session even when in a 0-peer limbo state.
4. **Chat Bubble Text Spilling Out on Mobile Keyboard Open**: On mobile viewports (e.g. iOS Safari), opening the virtual keyboard compresses the vertical space of `.messages-container`. Because `.message-bubble` lacks `flex-shrink: 0`, flexbox forcibly compresses each bubble down to its `min-height: 44px`. Because `overflow` is visible, the message text (`.message-text`) spills out below the bubble background onto the dark page background.
5. **Redundant Static Ellipses in Ticking System Messages**: Transient system status displays like `"Waiting for host..."` include three literal trailing dots in the input string, while the `.ticking-dots` element adds an additional 1 to 3 animated dots, producing an erroneous `"Waiting for host......"` (6 dots). All ellipsis messages must strictly display their dynamic ticking ellipsis with zero static dots.
6. **Mobile Browser Web Share API & Dynamic Share Icon**: On mobile browsers, sending invite links via the system share sheet (`navigator.share`) is significantly faster and more intuitive than manual copy-paste. When `navigator.share` is supported, clicking the bottom `#input-add-peer-btn` or the full-width `#btn-create-room` must trigger the native share sheet with the invite link, and the Stage 2 waiting button must dynamically morph to a Share icon (`SHARE_ICON_SVG`) and `aria-label="Share invite link"`, with re-clicking re-opening the share sheet. On desktop or unsupported platforms, the button gracefully falls back to clipboard copying with the Copy icon (`COPY_ICON_SVG`).
7. **Favicon Badge Outer Border Rounding**: The SVG favicon defined in `<link rel="icon">` previously used a square-ish rectangle (`rx='8'`), contrasting with the application-wide design system where badges and icons use fully rounded geometry (`border-radius: 9999px`). The outer border of the favicon badge must be smoothly rounded into a circular badge (`rx='15'` / `rx='16'`) with cleanly contained stroke.

---

## 2. Requirements & Acceptance Criteria

### REQ-043.1: Bottom Button On-Hover Glow Clipping Fix
- In `zero_client/style.css`:
  - Update `.message-form` and `.input-dock`:
    - Remove or override `overflow-x: hidden;` so that `overflow: visible;` allows box-shadows to bloom in 360 degrees.
    - Provide sufficient vertical clearance (e.g. `padding-top: 4px;` on `.message-form` or container margin) so that `transform: translateY(-1px)` and `box-shadow: 0 0 10px ...` never clip against parent boundaries.
    - Ensure `#send-button:hover` and `#input-add-peer-btn:hover` render seamless, uniform circular glows with no dark patches or linear clipping at the top.

### REQ-043.2: Burn Button Theme & Geometry Alignment with Leave Button
- In `zero_client/style.css`:
  - Update `#destroy-session-button` / `.btn-header-burn`:
    - Match `#header-leave-btn` vertical height: `height: 26px;` (aligned with leave button's 26px height).
    - Match `#header-leave-btn` color palette:
      - Default: `background: rgba(244, 63, 94, 0.12); border: 1px solid rgba(244, 63, 94, 0.32); color: #fda4af;`
      - Hover: `background: rgba(244, 63, 94, 0.24); border-color: rgba(244, 63, 94, 0.6); color: #ffffff; box-shadow: 0 0 10px rgba(244, 63, 94, 0.35); transform: translateY(-1px);`
      - Active: `transform: translateY(0);`
    - Geometry: `border-radius: 9999px; min-width: 80px; padding: 0 14px;` (wide oval pill).

### REQ-043.3: In-Room Burn Button Visibility Invariant (Limbo Parity with Leave Button)
- In `zero_client/index.html`:
  - In `updatePeerCountDisplay()`:
    - Whenever `inRoom` is true (whether `count === 0` in limbo or `count > 0` in active chat), if `headerLeaveBtn` is visible, `#destroy-session-button` MUST also be visible (`burnBtn.classList.remove("hidden")`).
    - In Home view (`!inRoom` / `resetToHome()`): both `headerLeaveBtn` and `burnBtn` are hidden (`classList.add("hidden")`).
  - In `handlePeerDeparture()`, `initEngineListeners()`, and room join flows: verify `burnBtn` stays visible in limbo when all peers depart, matching `headerLeaveBtn`.

### REQ-043.4: Mobile Chat Bubble Flex Containment (`flex-shrink: 0`)
- In `zero_client/style.css`:
  - On `.message-bubble`:
    ```css
    .message-bubble {
      flex-shrink: 0;
      flex: 0 0 auto;
      height: auto;
    }
    ```
  - When `.messages-container` height shrinks due to virtual keyboard opening, `.message-bubble` MUST NEVER shrink below its intrinsic content height (wrapping both `.message-header` and `.message-text`).
  - Text content (`.message-text`) remains 100% contained within the bubble background and border.

### REQ-043.5: Ellipsis De-duplication in System Messages
- In `zero_client/index.html`:
  - Update `showTransientStatus(text)`:
    - Defensively strip any trailing dots from `text` (`text.replace(/\.+$/, "")`) before appending `<span class="ticking-dots"></span>`.
    - Update all call sites (e.g. `showTransientStatus("Waiting for host")`) to omit literal dots.
  - Update `appendTemporarySystemMessage(text, isWaitingDots)`:
    - If `isWaitingDots` is true, defensively strip any trailing dots from `text`.
  - All status/temporary messages render strictly with dynamic animated ticking dots and 0 redundant static dots.

### REQ-043.6: Mobile Browser Web Share API Integration and Stage 2 Dynamic Share Icon
- In `zero_client/index.html`:
  - Define `SHARE_ICON_SVG`:
    ```javascript
    const SHARE_ICON_SVG = `
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="18" cy="5" r="3"></circle>
            <circle cx="6" cy="12" r="3"></circle>
            <circle cx="18" cy="19" r="3"></circle>
            <line x1="8.59" y1="13.51" x2="15.42" y2="17.49"></line>
            <line x1="15.41" y1="6.51" x2="8.59" y2="10.49"></line>
        </svg>`.trim();
    ```
  - Feature detection:
    `const canShare = typeof navigator !== "undefined" && typeof navigator.share === "function";`
  - When `canShare` is true:
    - On clicking `#btn-create-room` (in `createRoomAsHost()` or `handleInRoomInvite()`), or `#input-add-peer-btn` (in `handleInRoomInvite()`):
      - Invoke `navigator.share({ title: "zero-chat", text: "Join my encrypted zero-chat session:", url: inviteUrl })` (silently swallowing `AbortError` if user dismisses native share dialog).
      - Maintain background clipboard write (`navigator.clipboard.writeText`) as a fallback so the invite link is also in clipboard.
      - In Stage 2 (`setCreateRoomStage2()` and `setInputAddPeerBtnStage2()`):
        - Set button innerHTML to `SHARE_ICON_SVG` instead of `COPY_ICON_SVG`.
        - Set `aria-label` to `"Share invite link"` instead of `"Copy invite link"`.
      - On subsequent clicks while in Stage 2 (waiting for peer):
        - Re-invoke `navigator.share(...)` and background clipboard write.
  - When `canShare` is false (desktop browsers or browsers without Web Share support):
    - Maintain existing behavior: clipboard copy, `COPY_ICON_SVG`, and `aria-label="Copy invite link"`.

### REQ-043.7: Favicon Badge Outer Border Rounding
- In `zero_client/index.html`:
  - Update `<link rel="icon">` SVG data URI:
    - Replace the squarish `<rect width='32' height='32' rx='8' ...>` with a fully rounded circular badge:
      `<rect x='1' y='1' width='30' height='30' rx='15' fill='%23121826' stroke='%2338bdf8' stroke-width='2' stroke-opacity='0.4'/>`
    - Ensures the 2px cyan border (`#38bdf8`) has 100% round curvature matching the application-wide pill/circle design language (`border-radius: 9999px`) without clipping against the viewBox boundary.

### REQ-043.8: Automated Headless Browser & Workspace Test Verification
- In `zero_client/tests/browser/test_mesh_scenario.js`:
  - Update Test 18 and Test 19 to assert:
    - `.message-bubble` has `flex-shrink: 0`.
    - `#destroy-session-button` theme matches `rgba(244, 63, 94, ...)` and `#fda4af`.
    - `#destroy-session-button` remains visible during 0-peer limbo room state when `#header-leave-btn` is visible.
    - `showTransientStatus` strips trailing dots and never produces 6 dots.
  - Add Test 20 to assert:
    - When `navigator.share` is present: `#btn-create-room` and `#input-add-peer-btn` invoke `navigator.share`, display `SHARE_ICON_SVG` in Stage 2, set `aria-label="Share invite link"`, and subsequent clicks re-invoke `navigator.share`.
    - When `navigator.share` is absent: `#btn-create-room` and `#input-add-peer-btn` copy to clipboard, display `COPY_ICON_SVG` in Stage 2, and set `aria-label="Copy invite link"`.
    - Favicon `<link rel="icon">` has rounded outer border (`rx='15'` or circular geometry).
  - All browser test suites pass (`bash zero_client/tests/browser/run_test.sh`).
  - All 123 Rust workspace tests pass (`cargo test --workspace`).
  - Dual WASM compilation checks pass (`zero_client` and `zero_network`).

---

## 3. Target Architecture & Affected Modules

| Module / File | Scope of Changes |
|---|---|
| `zero_client/style.css` | Fix bottom button hover glow clipping (`overflow: visible`, padding); align burn button color palette/height with leave button (`rgba(244, 63, 94, ...)`, 26px); add `flex-shrink: 0` to `.message-bubble`. |
| `zero_client/index.html` | Ensure `#destroy-session-button` is visible whenever `#header-leave-btn` is visible (including limbo); clean static dots from `showTransientStatus`; integrate Web Share API (`navigator.share`) on mobile browsers for `#btn-create-room` and `#input-add-peer-btn`, morphing Stage 2 to dynamic `SHARE_ICON_SVG` and `aria-label="Share invite link"`; round outer border of favicon badge in `<link rel="icon">`. |
| `zero_client/tests/browser/test_mesh_scenario.js` | Update test assertions for limbo burn button visibility, burn button color palette, bubble flex containment, ellipsis de-duplication, and Web Share API / Share icon behavior. |
| `openspec/specs/overview.md` | Reconcile canonical spec with Invariant 18 & 19 updates upon completion. |

---

## 4. Verification & Quality Gates

1. **Gate 1: Rust Unit & Integration Tests**: `cargo test --workspace` (all tests pass).
2. **Gate 2: Dual WASM Compilation**:
   - `cargo check --target wasm32-unknown-unknown -p zero_client`
   - `cargo check --target wasm32-unknown-unknown -p zero_network`
3. **Gate 3: Formatting & Linting**:
   - `cargo fmt -- --check`
   - `cargo clippy --workspace --all-targets -- -D warnings`
4. **Gate 4: Headless Browser Regression Suite**:
   - `bash zero_client/tests/browser/run_test.sh` (all 19 tests pass).
5. **Gate 5: Architectural & Security Review**: Independent Reviewer evaluation with `APPROVE` verdict.
