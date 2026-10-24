# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-038"
title: "Align Chat Bubble Coloured Dot with Burn Button Icon"
state: "COMPLETED"
created_at: "2026-10-24T23:30:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-038/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

In chat message bubbles (`.message-bubble`), the left-side colored identity dot (`.peer-dot` / `.self-dot`) is vertically misaligned relative to the right-side flame burn button (`.btn-message-burn`).

### Root Cause Analysis
1. **Uncoupled Geometry**: `.btn-message-burn` was historically implemented with `position: absolute; top: 6px; right: 8px; width: 22px; height: 22px;`. The vertical center of the flame SVG icon is located at `y = 6px + 11px = 17px` relative to the top border of the bubble.
2. **Disconnected Header Flow**: Following the removal of sender labels and timestamps in ticket-037, `.message-header` contains only the 7px dot (`.peer-dot` / `.self-dot`). Because `.message-bubble` has `padding: 8px 12px;`, the top of `.message-header` sits at `y = 8px`. The 7px dot's vertical center is at `y = 8px + 3.5px = 11.5px`.
3. **5.5px Vertical Drift**: The colored dot sits ~5.5px higher than the flame burn button icon, creating an awkward, uneven header row inside every incoming and outgoing chat bubble.

### Objective
Unify `.message-header` into a single flex container row that houses both the colored dot on the left and `.btn-message-burn` on the right, ensuring their vertical centers are strictly aligned along the exact same horizontal axis (`align-items: center;`).

---

## 2. Requirements & Acceptance Criteria

### REQ-038.1: Unified Flex Message Header & Vertical Alignment
- In `zero_client/index.html` `appendMessageBubble`:
  - Mount `.btn-message-burn` directly into `.message-header` (after the dot), rather than as a direct child of `.message-bubble`.
  - The DOM structure for a chat bubble shall be:
    ```html
    <div class="message-bubble message-outgoing" data-message-id="...">
      <div class="message-header">
        <span class="peer-dot self-dot"></span>
        <button type="button" class="btn-message-burn" aria-label="Burn message">
          <svg ...>...</svg>
        </button>
      </div>
      <div class="message-text">hello</div>
    </div>
    ```
- In `zero_client/style.css`:
  - `.message-header`:
    - Define `display: flex; align-items: center; justify-content: space-between; width: 100%; min-height: 22px; margin-bottom: 2px;`.
  - `.btn-message-burn`:
    - Remove `position: absolute; top: 6px; right: 8px;`.
    - Set `position: relative; margin-left: auto; width: 22px; height: 22px; display: inline-flex; align-items: center; justify-content: center; flex-shrink: 0;`.
    - Retain existing transparent background, clean icon hover effect (`color: #fca5a5; transform: scale(1.1);`), and active effect (`color: #f87171; transform: scale(0.95);`).
  - `.peer-dot`, `.self-dot`:
    - Retain `width: 7px; height: 7px; border-radius: 50%; display: inline-block; flex-shrink: 0;`.
    - Both dot and flame SVG are vertically centered at `11px` within the 22px header.

### REQ-038.2: Outgoing & Incoming Header Symmetry
- In `zero_client/style.css`:
  - Ensure `.message-outgoing .message-header` and `.message-incoming .message-header` both support the space-between flex layout with no conflicting alignment overrides.
  - The left colored dot and right burn button icon must align identically across outgoing (`--accent-emerald`) and incoming (`--peer-color`) bubbles.

### REQ-038.3: Regression & Automated Test Verification
- In `zero_client/tests/browser/test_mesh_scenario.js`:
  - Update / add assertions in Test 15 and Test 17:
    1. Verify `outBubble.querySelector(".message-header .btn-message-burn") !== null` and `inBubble.querySelector(".message-header .btn-message-burn") !== null`.
    2. Verify `outBubble.querySelector(".message-header .peer-dot") !== null`.
    3. Verify clicking `.btn-message-burn` when nested in `.message-header` cleanly dissolves the message and broadcasts `BurnMessage` without event propagation issues.
  - Verify all 17 browser headless tests pass cleanly (`bash zero_client/tests/browser/run_test.sh`).
  - Verify all 123 Rust workspace tests pass (`cargo test --workspace`).

---

## 3. Target Architecture & Affected Modules

| Module / File | Changes & Responsibilities |
|---|---|
| `zero_client/index.html` | Mount `burnBtn` inside `header` instead of directly on `bubble` in `appendMessageBubble()`. |
| `zero_client/style.css` | 1. Update `.message-header` to `display: flex; align-items: center; justify-content: space-between; width: 100%; min-height: 22px;`.<br>2. Remove `position: absolute; top: 6px; right: 8px;` from `.btn-message-burn`, adding `margin-left: auto; position: relative;`. |
| `zero_client/tests/browser/test_mesh_scenario.js` | Assert that `.btn-message-burn` resides within `.message-header` and functions identically. |
| `openspec/specs/overview.md` | Document header flex alignment invariant upon completion. |

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
