# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-010"
title: "System Message Styling & Lifecycle Events, Deactivated Input Bar States"
state: "IMPLEMENTED"
roadmap: "serverless-p2p-chat"
depends_on: ["ticket-009"]
created_at: "2026-10-17T15:47:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-010/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/serverless-p2p-chat.md`

## 1. Problem Statement & Objective
In the current ZeroChat UI, system notifications (such as "Secure room created.") are rendered using standard incoming message bubbles complete with header tags ("SYSTEM"), timestamps, and ChaCha20 cryptographic verification badges. System lifecycle events should instead be rendered as simple, centered, italic text without bubbles or badges. Furthermore, additional lifecycle announcements ("Peer joined.", "Joined room.", "Peer left.") are missing. Lastly, the message input bar and Send button appear identical to their active state even when a P2P session is disconnected/inactive, with hover effects still firing on the disabled Send button.

The objective of this task is to:
1. Redesign system messages as simple, italic, centered text outside of message bubbles.
2. Add "Secure room created.", "Peer joined.", "Joined room.", and "Peer left." lifecycle messages.
3. Visually distinguish the message input and Send button when inactive, disabling hover effects on Send when deactivated.

## 2. Scope Boundaries
- **In-Scope**:
  - `zero_client/index.html`:
    - Add `appendSystemMessage(text)` DOM helper.
    - Emit "Secure room created." on host room creation.
    - Emit "Joined room." for joiner upon DataChannel open (`st === "open"`).
    - Emit "Peer joined." for host upon DataChannel open (`st === "open"`).
    - Emit "Peer left." when DataChannel closes (`st === "closed"`).
  - `zero_client/style.css`:
    - Add `.system-message` styling (centered, italic, muted text, no bubble).
    - Add `.message-input:disabled` styling (muted background, dimmed placeholder, not-allowed cursor).
    - Add `.btn-send:disabled` and `.btn-send:disabled:hover` styling (flat slate, no shadow, no hover transform, not-allowed cursor).
  - Workspace test suite:
    - Ensure all tests pass without regressions.
- **Out-of-Scope**:
  - "Left room." requirement (dropped per user alignment).
  - WebRTC signaling backend (`zero_network`).

## 3. Requirements & Acceptance Criteria
- [x] **REQ-010.1**: System messages render as simple, italic, centered text without message bubble containers, headers, or crypto badges.
- [x] **REQ-010.2**: When a room is created, "Secure room created." displays in the updated system message style.
- [x] **REQ-010.3**: When a peer connects to an existing room, existing participants see "Peer joined." in the updated system message style.
- [x] **REQ-010.4**: When a joiner connects to a room, the joiner sees "Joined room." in the updated system message style.
- [x] **REQ-010.5**: When a peer disconnects/leaves, the remaining participant sees "Peer left." in the updated system message style.
- [x] **REQ-010.6**: The message input bar and Send button visually indicate their deactivated state when not in an active chat, and the Send button has no hover animation or glow when disabled.
- [x] **REQ-010.7**: All workspace tests pass (`cargo test --workspace`).

## 4. Architectural Design & Impacted Modules
- `zero_client/index.html`: DOM helper and event bindings.
- `zero_client/style.css`: CSS styling for system messages and disabled inputs.
