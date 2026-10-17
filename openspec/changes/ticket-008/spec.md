# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-008"
title: "UI Simplification: Header Streamlining, P2P Established Badge & Banner Cleanup"
state: "IMPLEMENTED"
roadmap: "serverless-p2p-chat"
depends_on: ["ticket-007"]
created_at: "2026-10-17T13:42:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-008/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/serverless-p2p-chat.md`

## 1. Problem Statement & Objective
Following the production deployment of ZeroChat, initial user testing indicates that the web interface contains developer-focused diagnostics and superfluous visual clutter that detracts from a clean, modern user experience. Specifically, the Diagnostics link, the technical "P2P Direct (Edge Severed)" badge, the redundant "Join with Link" input controls, the static `[ZERO PERSISTENCE]` banner card, and boilerplate headers ("Direct P2P Room") should be removed or refined. The peer connection indicator should be streamlined to "P2P Established".

The objective of this task is to strip away all unnecessary visual noise, refine the header and room creation controls, and ensure that joining via URL fragments (`#payload=...`) continues to work seamlessly with zero cognitive friction.

## 2. Scope Boundaries
- **In-Scope**:
  - `zero_client/index.html`:
    - Remove the "Diagnostics" button/link.
    - Remove the "P2P Direct (Edge Severed)" / "Disconnected" status pill from the header.
    - Update peer status pill wording to "P2P Established" (`🟢 P2P Established`).
    - Remove the static `[ZERO PERSISTENCE]` alert banner from `#messages-container`.
    - Remove the "Join with Link" toggle button and paste input box completely.
    - Remove the "Direct P2P Room" title and "Create an ephemeral room or join with an invite link" subtitle.
  - `.github/workflows/deploy.yml` & `scripts/deploy_dry_run.sh`:
    - Clean up `test_harness.html` from deployment packaging artifacts.
  - Test Suite:
    - Ensure all 91 unit and integration tests continue to pass without warnings or regressions.
- **Out-of-Scope**:
  - WebRTC signaling backend (`zero_network`).
  - Wire protocol envelopes (`zero_protocol`).
  - Cryptographic algorithms (`crypto.rs`).

## 3. Requirements & Acceptance Criteria
- [x] **REQ-008.1**: The "Diagnostics" button is completely removed from the user interface.
  - *Verification*: Inspect DOM in browser to ensure no diagnostics link or button exists.
- [x] **REQ-008.2**: The "P2P Direct (Edge Severed)" status indicator pill is removed from the header.
  - *Verification*: Header contains only brand logo/title, peer counter, active peer badge, and "Burn Session" button.
- [x] **REQ-008.3**: When peers connect over WebRTC DataChannel, the badge displays "🟢 P2P Established".
  - *Verification*: Connected state renders `<span class="peer-pill">🟢 P2P Established</span>`.
- [x] **REQ-008.4**: The static `[ZERO PERSISTENCE]` alert card is completely removed from the messages feed.
  - *Verification*: Message feed starts empty, ready for real-time chat messages.
- [x] **REQ-008.5**: The "Join with Link" toggle button and input group are completely removed.
  - *Verification*: Room controls contain only "✨ Create Private Room" (and the generated copyable link once created). Automatic URL fragment joining remains fully functional when loading or navigating to `#payload=...`.
- [x] **REQ-008.6**: The "Direct P2P Room" header title and descriptive subtitle are completely removed.
  - *Verification*: The invite/room section is compact and minimal.
- [x] **REQ-008.7**: All workspace tests pass and compilation remains warning-free.
  - *Verification*: `cargo test --workspace` passes (91/91 tests).

## 4. Architectural Design & Impacted Modules
- **Modified Modules**:
  - `zero_client/index.html`: Streamlined layout and simplified event handling.
  - `.github/workflows/deploy.yml`: Remove `test_harness.html` static bundle step.
  - `scripts/deploy_dry_run.sh`: Remove `test_harness.html` bundle step.

## 5. Decomposition Plan (Architect Briefs)

| Batch | Brief ID | Role | Target Thinking Level | Description | Files Scope | Dependencies |
| :---: | :--- | :--- | :---: | :--- | :--- | :--- |
| **1** | `brief-01-ui-streamline` | Implementer | `LOW` | Remove diagnostics, status pill, zero persistence card, join-with-link input, and boilerplate text. Update badge to 'P2P Established'. | `zero_client/index.html`, `.github/workflows/deploy.yml`, `scripts/deploy_dry_run.sh` | None |
| **2** | `brief-val-tests` | Validator | `LOW` | Validate full workspace test suite and clippy linters | Workspace | Batch 1 |
| **3** | `brief-rev` | Reviewer | `LOW` | Verify UI cleanliness, zero regressions, and user specification compliance | Candidate diff | Batch 2 |
