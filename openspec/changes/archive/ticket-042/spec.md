# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-042"
title: "UI Architecture Refinements: Transparent Panels, Relocated Header Burn Button, and Rounded Controls"
state: "COMPLETED"
created_at: "2026-10-25T14:55:00Z"
completed_at: "2026-10-25T15:35:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-042/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

The application UI currently exhibits several visual inconsistencies and sub-optimal spatial layouts across both desktop and mobile viewports:
1. **Unnecessary Panel Backgrounds**: The top bar (`.chat-header`), splash screen (`.welcome-splash`), and bottom footer dock (`.chat-footer`, `#create-room-bar`) have distinct panel backgrounds (`rgba(18, 24, 38, 0.7)`), creating hard boxed borders and color disparity against the dark ambient gradient background.
2. **Top-Bar Clutter in Active Chat**: The text `"zero-chat.pages.dev"` and tagline `"SECURE P2P CHAT"` remain visible during active chat, crowding header controls and consuming valuable horizontal space.
3. **Dedicated Burn Row Consuming Vertical Height**: The burn chat button currently sits in a separate full-width row (`#burn-room-bar`) underneath the header, pushing messages down and creating visual fragmentation.
4. **Bottom Bar Button & Input Box Geometry**: Bottom bar buttons (`#input-add-peer-btn` and `#send-button`) have boxy 8px border-radii instead of matching the rounded glass aesthetic of top bar controls. Furthermore, the footer container has an opaque background while the text input box has a darker inset background.

### User Intent & Core Principles
- **Atmospheric Transparency**: Remove the background color, borders, and box-shadows of `.chat-header`, `.welcome-splash`, and `.chat-footer` / `#create-room-bar` so the background gradient flows seamlessly through the interface.
- **Brand Minimalism in Active Chat**: Display the domain title (`zero-chat.pages.dev`) and subtitle (`SECURE P2P CHAT`) strictly in the Home view. In active chat, show only the cyan `0` brand glyph on the top-left, freeing up horizontal header space.
- **Header-Integrated Burn Button**: Move the burn chat button (`#destroy-session-button`) into the top-right header row (`.header-controls`) as the leftmost button of that row (positioned to the left of the peer count badge, status indicator, and leave button). Give it a significantly wider oval shape that matches the top-bar button style and takes advantage of the freed-up header space, eliminating the `#burn-room-bar` row entirely.
- **Rounded Bottom Bar Buttons & Top-Bar Styling**: Make bottom bar buttons (`#input-add-peer-btn` and `#send-button`) rounded (pill/circle `border-radius: 9999px`) and style them using the top-bar button translucent glass language.
- **Inverted Dock & Input Styling**: Make the bottom bar dock background completely transparent; style `#message-input` with the previous bottom bar panel background (`rgba(18, 24, 38, 0.7)`), `backdrop-filter: blur(16px)`, rounded pill border-radius, and subtle border.

---

## 2. Requirements & Acceptance Criteria

### REQ-042.1: Transparent Background Top Bar
- In `zero_client/style.css`:
  - Set `.chat-header`:
    ```css
    .chat-header {
      background: transparent !important;
      border: none !important;
      box-shadow: none !important;
      backdrop-filter: none !important;
      -webkit-backdrop-filter: none !important;
    }
    ```
  - Remove any background color difference between the top bar and the ambient page background.

### REQ-042.2: Transparent Background Splash Screen
- In `zero_client/style.css`:
  - Set `.welcome-splash`:
    ```css
    .welcome-splash {
      background: transparent !important;
      border: none !important;
      box-shadow: none !important;
      backdrop-filter: none !important;
      -webkit-backdrop-filter: none !important;
    }
    ```
  - Features inside `.welcome-features` retain their readable layout against the background.

### REQ-042.3: Transparent Background for Create New Chat & Limbo Bottom Bar
- In `zero_client/style.css` and `zero_client/index.html`:
  - The bottom footer container `.chat-footer` and `#create-room-bar`:
    ```css
    .chat-footer {
      background: transparent !important;
      border: none !important;
      box-shadow: none !important;
      backdrop-filter: none !important;
      -webkit-backdrop-filter: none !important;
    }
    .create-room-bar {
      background: transparent !important;
      border: none !important;
      box-shadow: none !important;
    }
    ```
  - `#btn-create-room.btn-emerald` preserves its vivid emerald color (`--accent-emerald` gradient and glow) in both Home View (`Create Chat`) and Limbo Disconnect view (`+` Add peer).

### REQ-042.4: Active Chat Top-Bar Brand Visibility
- In `zero_client/index.html`:
  - In Home view (`!inRoom`): `.brand-text` (`.brand-title` and `.brand-tagline`) is visible alongside `.brand-icon` (`0`).
  - In Room / Active chat view (`inRoom`): `.brand-text` is hidden (`classList.add("hidden")`), leaving only the brand glyph `0` on the left.
  - On `resetToHome()`: `.brand-text` is unhidden (`classList.remove("hidden")`).

### REQ-042.5: Relocate Burn Button to Top-Right Header as a Wide Oval Button
- In `zero_client/index.html`:
  - Move the burn button (`#destroy-session-button`) inside `.header-controls` as the leftmost element of that row (positioned before `#peer-count`, `#idle-status-capsule`, `#peer-list`, and `#header-leave-btn`).
  - Delete or omit `#burn-room-bar`, eliminating the extra vertical row underneath the header.
- In `zero_client/style.css`:
  - Style `#destroy-session-button.btn-header-burn`:
    - Adopts the top-bar button visual style (translucent glass, subtle border, smooth hover/active transitions).
    - Shaped as a significantly wider oval:
      ```css
      #destroy-session-button.btn-header-burn {
        min-width: 80px;
        height: 28px;
        padding: 0 14px;
        border-radius: 9999px;
        background: rgba(239, 68, 68, 0.12);
        border: 1px solid rgba(239, 68, 68, 0.3);
        color: var(--accent-rose);
        display: inline-flex;
        align-items: center;
        justify-content: center;
        gap: 6px;
        transition: all 0.2s ease;
      }
      #destroy-session-button.btn-header-burn:hover {
        background: rgba(239, 68, 68, 0.24);
        border-color: rgba(239, 68, 68, 0.6);
        color: #ffffff;
        box-shadow: 0 0 10px rgba(239, 68, 68, 0.35);
        transform: translateY(-1px);
      }
      ```
- In `zero_client/index.html`:
  - Managed by `updatePeerCountDisplay()`:
    - In Home view (`!inRoom`): hidden.
    - In 0-peer room view (`inRoom && openPeerCount === 0`): hidden.
    - In active chat (`inRoom && openPeerCount > 0`): visible (`classList.remove("hidden")`).

### REQ-042.6 & REQ-042.7: Rounded Bottom Bar Buttons with Top-Bar Styling
- In `zero_client/style.css`:
  - `#input-add-peer-btn` (`.btn-action-peer`) and `#send-button` (`.btn-send-emerald`):
    - `border-radius: 9999px;` (fully rounded / pill).
    - Dimensions: 42px × 42px circular or pill shaped.
    - Adopt top bar button styling:
      - Translucent glass background with subtle borders and micro-glow hover states.
      - `#input-add-peer-btn`: Stage 1 cyan translucent (`background: rgba(56, 189, 248, 0.12); border: 1px solid rgba(56, 189, 248, 0.3); color: var(--accent-cyan);`), Stage 2 deeper cyan (`background: rgba(56, 189, 248, 0.28); border-color: rgba(56, 189, 248, 0.65); color: #38bdf8;`).
      - `#send-button`: emerald translucent (`background: rgba(16, 185, 129, 0.15); border: 1px solid rgba(16, 185, 129, 0.35); color: var(--accent-emerald);`), hover (`background: rgba(16, 185, 129, 0.28); border-color: rgba(16, 185, 129, 0.65); box-shadow: 0 0 10px rgba(16, 185, 129, 0.35); color: #ffffff;`), disabled (`background: rgba(255, 255, 255, 0.03); border: 1px solid rgba(255, 255, 255, 0.06); color: #475569;`).

### REQ-042.8: Transparent Dock & Input Background Adoption
- In `zero_client/style.css`:
  - `.message-form, .input-dock`:
    - `background: transparent !important;`
  - `#message-input, .message-input`:
    - Adopts the bottom dock panel background:
      ```css
      background: rgba(18, 24, 38, 0.7);
      backdrop-filter: blur(16px);
      -webkit-backdrop-filter: blur(16px);
      border: 1px solid rgba(255, 255, 255, 0.12);
      border-radius: 9999px;
      padding: 10px 18px;
      font-size: 16px;
      ```
    - Focus state: `border-color: rgba(56, 189, 248, 0.5); box-shadow: 0 0 12px rgba(56, 189, 248, 0.2);`.

### REQ-042.9: Automated Regression Test Suite
- In `zero_client/tests/browser/test_mesh_scenario.js`:
  - Update tests to verify:
    - Top bar `.chat-header` transparent styling.
    - Splash screen `.welcome-splash` transparent styling.
    - Footer `.chat-footer` transparent styling.
    - Active chat hides `.brand-text`, home view displays `.brand-text`.
    - `#destroy-session-button` located in `.header-controls` as the leftmost element.
    - Rounded buttons and pill input styling.
  - All 123 Rust workspace tests pass (`cargo test --workspace`).
  - Dual WASM checks pass (`zero_client` and `zero_network`).
  - Headless browser test suite passes (`bash zero_client/tests/browser/run_test.sh`).

---

## 3. Target Architecture & Affected Modules

| Module / File | Scope of Changes |
|---|---|
| `zero_client/style.css` | Transparent backgrounds for header, splash, footer; wide oval burn button in header; rounded bottom buttons adopting top-bar glass styling; `#message-input` adopting dock panel background and pill border-radius. |
| `zero_client/index.html` | Move burn button into `.header-controls`; toggle `.brand-text` visibility between home and room states; wire burn button visibility in header controls. |
| `zero_client/tests/browser/test_mesh_scenario.js` | Update regression test assertions for header burn button location, brand text visibility, and CSS transparency. |
| `openspec/specs/overview.md` | Reconcile canonical spec with Invariant 18 upon task completion. |

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
   - `bash zero_client/tests/browser/run_test.sh` (all tests pass).
5. **Gate 5: Architectural & Security Review**: Reviewer verification with `APPROVE` verdict.
