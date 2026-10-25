# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-044"
title: "UI Architecture: Desktop Share Bypass, Clean Mobile Share URL, and Chat Bubble Burn Icon Optical Alignment"
state: "PLANNING"
created_at: "2026-10-25T17:35:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-044/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

Following the delivery of `ticket-043`, three critical UI behaviors require refinement across desktop and mobile platforms:

1. **Unwanted OS Share Popover on Desktop Browsers**: Modern desktop browsers on macOS (Chrome, Safari, Edge) and Windows 10+ implement the W3C Web Share API (`navigator.share`) by displaying the desktop OS system sharing menu (AirDrop, Mail, Messages, Notes). Because `ticket-043` evaluated only `typeof navigator.share === "function"`, clicking the bottom action buttons on desktop triggers the desktop OS share menu instead of instantaneously copying the invite URL to the clipboard. Desktop users expect direct, zero-friction clipboard copying.
2. **Mobile Share Sheet "Copy" Action Prepends Text to URL**: In `ticket-043`, `navigator.share` was invoked with `{ title, text, url }`, where `text` was `"Join my encrypted zero-chat session:"`. On iOS (Safari) and Android (Chrome), tapping the "Copy" action inside the native OS share sheet concatenates the text and the URL into a single clipboard string: `"Join my encrypted zero-chat session: https://zero-chat.pages.dev/#payload=..."`. When users attempt to paste this into their mobile browser's address bar to follow the invite, the browser treats the prepended English words as a search query rather than navigating to the hash-fragment URL, preventing session join.
3. **In-Chat Bubble Flame Burn Icon Optical Misalignment**: Within `.message-header`, the left identity dot (`.peer-dot` / `.self-dot`) is a 7px circle centered at $y = 11\text{px}$. The right flame burn icon (`.btn-message-burn`) has a teardrop geometry with 80% of its visual mass in the lower bulb ($y \approx 13.5\text{px}$). Because of this bottom-heavy shape, the flame icon visually sits ~2px lower than the identity dot. Elevating the button by 2px (`top: -2px;`) restores perfect horizontal optical balance.

---

## 2. Requirements & Acceptance Criteria

### REQ-044.1: Desktop Web Share Bypass & Robust Mobile-Only Detection
- In `zero_client/index.html`:
  - Implement a dedicated helper function `isMobileBrowser()`:
    ```javascript
    function isMobileBrowser() {
        if (typeof navigator === "undefined") return false;
        if (navigator.userAgentData && typeof navigator.userAgentData.mobile === "boolean") {
            return navigator.userAgentData.mobile;
        }
        const ua = navigator.userAgent || "";
        const isMobileUA = /Mobi|Android|iPhone|iPad|iPod/i.test(ua);
        const isTouchMac = Boolean(navigator.maxTouchPoints && navigator.maxTouchPoints > 2 && /Macintosh/.test(ua));
        return isMobileUA || isTouchMac;
    }
    ```
  - Scope `canShare`:
    ```javascript
    const canShare = isMobileBrowser() && typeof navigator !== "undefined" && typeof navigator.share === "function";
    ```
  - **Desktop Invariant**: On all desktop browsers (macOS, Windows, Linux, non-touch devices):
    - `canShare` evaluates to `false`.
    - Clicking `#btn-create-room` or `#input-add-peer-btn` NEVER triggers `navigator.share` or an OS share popover.
    - Directly writes the invite URL to clipboard via `navigator.clipboard.writeText`.
    - Stage 2 button displays `COPY_ICON_SVG` with `aria-label="Copy invite link"`.
    - Temporary system notice displays `"Invite link copied to clipboard"`.
  - **Mobile Invariant**: On mobile browsers (iOS Safari, Android Chrome, iPadOS):
    - When `navigator.share` is supported, `canShare` evaluates to `true`.
    - Stage 2 button displays `SHARE_ICON_SVG` with `aria-label="Share invite link"`.
    - Clicking opens the native mobile share sheet, and re-clicking in Stage 2 re-invokes the mobile share sheet.

### REQ-044.2: Clean URL-Only Mobile Share Payload
- In `zero_client/index.html`:
  - When invoking `navigator.share` in both `createRoomAsHost()` and `handleInRoomInvite()`:
    ```javascript
    await navigator.share({
        title: "zero-chat",
        url: inviteUrl
    });
    ```
  - Strictly omit the `text` field from the share payload.
  - Guarantees that when a user selects **"Copy"** inside the native mobile share sheet (iOS/Android), the clipboard receives **strictly and exclusively** the clean URL (`https://zero-chat.pages.dev/#payload=...`), without any leading description or text prefix.
  - Pasting into a mobile browser's address bar immediately navigates to and joins the session.

### REQ-044.3: In-Chat Bubble Flame Burn Icon Optical Alignment
- In `zero_client/style.css`:
  - On `.btn-message-burn`:
    - Add `top: -2px;` (preserving existing `position: relative;`).
    - Elevates the flame icon's visual center of mass to align in optical symmetry with the left `.peer-dot` / `.self-dot` ($y \approx 11.5\text{px}$).
    - Preserves all hover and active micro-interactions (`transform: scale(1.1)`, etc.) without altering the flex layout of `.message-header` or colliding with `.message-text`.

### REQ-044.4: Automated Test & Verification Gates
- In `zero_client/tests/browser/test_mesh_scenario.js`:
  - Update Test 20 to assert:
    - Desktop simulation (`isMobileBrowser() === false`): `#btn-create-room` and `#input-add-peer-btn` do NOT invoke `navigator.share`, copy clean URL to clipboard, display `COPY_ICON_SVG`, and set `aria-label="Copy invite link"`.
    - Mobile simulation (`isMobileBrowser() === true`): `#btn-create-room` and `#input-add-peer-btn` invoke `navigator.share` with strictly `{ title, url }` and `undefined` text.
    - `.btn-message-burn` defines `top: -2px;` in `style.css`.
  - All 20 browser tests pass (`bash zero_client/tests/browser/run_test.sh`).
  - All 123 Rust workspace tests pass (`cargo test --workspace`).
  - Dual WASM compilation checks pass (`zero_client` and `zero_network`).
  - `cargo fmt -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` pass cleanly.

---

## 3. Target Architecture & Affected Modules

| Module / File | Scope of Changes |
|---|---|
| `zero_client/index.html` | Implement `isMobileBrowser()`; scope `canShare` to mobile platforms; remove `text` field from `navigator.share` payload to ensure clean URL copying from OS share sheets. |
| `zero_client/style.css` | Add `top: -2px;` to `.btn-message-burn` to align optical center of mass with `.peer-dot`. |
| `zero_client/tests/browser/test_mesh_scenario.js` | Update Test 20 to verify desktop share bypass, clean URL share payload (no `text` attribute), and `.btn-message-burn` optical offset. |
| `openspec/specs/overview.md` | Reconcile canonical spec with Invariant 19 updates upon completion. |

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
   - `bash zero_client/tests/browser/run_test.sh` (all 20 tests pass).
5. **Gate 5: Architectural & Security Review**: Independent Reviewer evaluation with `APPROVE` verdict.
