# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-019"
title: "UI Tweaks: Non-Pulsing Blue Dot Idle Header Badge and Splash Card Wording Refinement"
state: "COMPLETED"
roadmap: "serverless-p2p-chat"
depends_on: ["ticket-018"]
created_at: "2026-10-18T16:47:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-019/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/serverless-p2p-chat.md`

## 1. Problem Statement & Objective
In the current ZeroChat UI:
1. **Idle Top-Bar Header Badge Color & Animation**:
   - The disconnected idle status capsule (`#idle-status-capsule`) currently displays a pulsing red dot inside a red glass capsule.
   - The pulsing animation and red color create an alarming or active-recording aesthetic during idle state.
   - The user requested changing this badge to a calm, non-pulsing blue dot badge matching the cyan/blue palette of the peer counter badge and interface theme.
2. **Splash Feature Card Copy Refinement**:
   - Feature Card 1 description currently reads: `"Private data channels with no intermediate relays or storage servers."`
   - The user requested streamlining the copy by removing the word `"data"`:
     `"Private channels with no intermediate relays or storage servers."`

---

## 2. UI & Design Specifications

### Header Controls Idle State (Disconnected):
```text
┌─────────────────────────────────────────────────────────────┐
│ [0] zero-chat.pages.dev                   [ 0 ] [  ● blue ] │
│     SECURE P2P CHAT                    peer-count  58px idle
│                                        (Badge 1)    capsule
└─────────────────────────────────────────────────────────────┘
```
- `#idle-status-capsule` has blue glass styling:
  - `background: rgba(56, 189, 248, 0.1);`
  - `border: 1px solid rgba(56, 189, 248, 0.25);`
  - Fixed dimensions: `58px` width, `26px` height, `border-radius: 9999px`.
- Inner dot (`.blue-dot`):
  - `background-color: var(--accent-cyan);` (`#38bdf8`)
  - `box-shadow: 0 0 6px rgba(56, 189, 248, 0.6);`
  - `animation: none;` (explicitly non-pulsing).

### Welcome Splash Feature Card 1:
```text
┌─────────────────────────────────────────────────────────────┐
│ ⚡ Direct P2P Connection                                     │
│    Private channels with no intermediate relays or          │
│    storage servers.                                         │
└─────────────────────────────────────────────────────────────┘
```

---

## 3. Scope Boundaries
- **In-Scope**:
  - `zero_client/index.html`:
    - Update `<span class="status-indicator-dot red-dot">` inside `#idle-status-capsule` to `<span class="status-indicator-dot blue-dot">`.
    - Update Card 1 description in static `#welcome-splash` to `"Private channels with no intermediate relays or storage servers."`.
    - Update Card 1 description in `WELCOME_SPLASH_HTML` template string to `"Private channels with no intermediate relays or storage servers."`.
  - `zero_client/style.css`:
    - Update `.status-capsule-disconnected` to blue glass palette (`background: rgba(56, 189, 248, 0.1); border: 1px solid rgba(56, 189, 248, 0.25);`).
    - Define `.status-capsule-disconnected .blue-dot` with blue color (`var(--accent-cyan)`), subtle glow (`box-shadow: 0 0 6px rgba(56, 189, 248, 0.6)`), and non-pulsing behavior (`animation: none;`).
  - Validation:
    - Run workspace test suite (`cargo test --workspace`) to confirm 91 tests pass.
    - Run workspace linter (`cargo clippy --workspace --all-targets -- -D warnings`) with 0 warnings.
- **Out-of-Scope**:
  - Connected state layout, button behaviors, WebRTC signaling, or backend logic.

---

## 4. Requirements & Acceptance Criteria
- [x] **REQ-019.1 (Non-Pulsing Blue Dot Idle Header Badge)**:
  - In disconnected / idle mode, `#idle-status-capsule` displays a cyan/blue translucent glass background (`rgba(56, 189, 248, 0.1)`) and subtle blue border (`rgba(56, 189, 248, 0.25)`).
  - The inner dot inside `#idle-status-capsule` displays a static cyan/blue dot (`#38bdf8`) with `animation: none;` and no opacity/scale pulsing.
  - The capsule retains exact dimensions (`58px x 26px`) to ensure seamless alignment when splitting into active badges on connect.
- [x] **REQ-019.2 (Splash Feature Card 1 Copy Refinement)**:
  - Feature Card 1 description must read exactly `"Private channels with no intermediate relays or storage servers."` in both static DOM (`#welcome-splash`) and dynamic JavaScript template (`WELCOME_SPLASH_HTML`).
- [x] **REQ-019.3 (Test Suite & Non-Regression Integrity)**:
  - All workspace unit and integration tests (`cargo test --workspace`) and clippy checks must pass cleanly.
