# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-013"
title: "UI Polish: Secure P2P Chat Branding, Chat Terminology, Disconnect Deduplication, and Local Peer Color Assignment"
state: "PLANNING"
roadmap: "serverless-p2p-chat"
depends_on: ["ticket-012"]
created_at: "2026-10-17T17:31:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-013/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/serverless-p2p-chat.md`

## 1. Problem Statement & Objective
Following recent UI layout improvements:
1. The brand header displays `EPHEMERAL P2P` beneath "ZeroChat", which should be updated to `SECURE P2P CHAT` for clearer value proposition.
2. The UI inconsistently refers to "room" instead of "chat" (e.g. "Create Private Room", "Burn Room", "Secure room created.", "Peer joined.", "Peer left."). All references should be standardized to "chat" terminology.
3. Upon WebRTC DataChannel/PeerConnection disconnect, multiple asynchronous closing events fire (`ch.onclose`, `ch.onerror`, `pc.oniceconnectionstatechange`), causing duplicate `"Peer left."` system messages to appear in the chat feed.
4. Chat bubbles currently show explicit text tags (`PEER` and `YOU`) above messages, which clutters the message presentation. The bubbles already have distinct outgoing/incoming alignments; removing these text tags will streamline readability.
5. In multi-peer or 1:1 scenarios, peers lack distinct visual differentiation. Each remote peer should be dynamically assigned a deterministic client-local color from a curated palette upon joining, applying an accent tint or indicator to their incoming message bubbles so participants are instantly identifiable.

The objective of this task is to implement these visual, messaging, and event-handling refinements across `zero_client/index.html` and `zero_client/style.css`.

## 2. Scope Boundaries
- **In-Scope**:
  - `zero_client/index.html`:
    - Brand tagline updated from `EPHEMERAL P2P` to `SECURE P2P CHAT`.
    - Replace all "room" user-facing copy with "chat":
      - Button: "Create Private Chat" (and dynamic JS resets).
      - Button: "Burn Chat".
      - System notifications: `"Secure chat created."`, `"Peer joined chat."`, `"Joined chat."`, `"Peer left chat."`.
      - Joiner banner & status messages: `"Found Chat Invite!..."`, `"Chat creation failed"`.
    - Event state guard in `ZeroWebRtcEngine._setState()` and `initEngineListeners()` to deduplicate `"Peer left chat."` notifications on connection termination.
    - Remove `message-sender` (`PEER` / `YOU`) text element from message bubbles in `appendMessageBubble()`.
    - Implement local peer color palette assignment (e.g. HSL or curated palette: cyan, amber, purple, emerald, rose) keyed by peer ID/connection order, and attach peer color classes/CSS variables to incoming bubbles.
  - `zero_client/style.css`:
    - Styling for message bubbles without sender labels (clean timestamp alignment and padding).
    - Color accent styling and CSS variables for dynamically assigned peer color identifiers.
    - Header brand tagline styling for `SECURE P2P CHAT`.
  - Workspace tests:
    - Verify all workspace unit and integration tests (`cargo test --workspace`) continue to pass.
- **Out-of-Scope**:
  - Protocol envelope modifications or crypto cipher changes in `crates/zero_protocol`.
  - Cloudflare Worker backend logic in `zero_network`.

## 3. Requirements & Acceptance Criteria
- [ ] **REQ-013.1 (Brand Tagline Update)**: The header brand tagline must display `SECURE P2P CHAT` in place of `EPHEMERAL P2P`.
- [ ] **REQ-013.2 (Consistent Chat Terminology)**: All occurrences of "room" in user-facing UI and notifications must be replaced with "chat":
  - "Create Private Chat" on the primary action button and all JS reset paths.
  - "Burn Chat" on the destruction action bar.
  - Lifecycle notifications: `"Secure chat created."`, `"Peer joined chat."`, `"Joined chat."`, and `"Peer left chat."`.
- [ ] **REQ-013.3 (Deduplicated Disconnect Notifications)**: When a peer disconnects, exactly one `"Peer left chat."` system message must be rendered. State transitions in `ZeroWebRtcEngine` must be guarded (`if (this.state === s) return;`) to prevent duplicate closed events from re-triggering notifications.
- [ ] **REQ-013.4 (Streamlined Message Bubbles without Sender Labels)**: The text labels `"PEER"` and `"YOU"` must be removed from the header of chat message bubbles. The header should retain the formatted timestamp.
- [ ] **REQ-013.5 (Client-Local Peer Color Differentiation)**: Incoming peers must be assigned a unique client-local color from a vibrant, cohesive palette upon arrival. Incoming message bubbles must reflect this peer color via accent border, glow, or indicator badge, enabling effortless peer identification without clutter.
- [ ] **REQ-013.6 (Test & Regression Validation)**: All unit and integration tests across the workspace (`cargo test --workspace`) must pass cleanly.

## 4. Architectural Design & Impacted Modules
- `zero_client/index.html`:
  - Tagline markup and terminology replacements throughout DOM strings and event handlers.
  - State deduplication guard in `_setState(s)` and disconnect handling.
  - Message bubble template refactoring in `appendMessageBubble()`.
  - Local peer color assignment map (`peerColors = new Map()`, `PEER_COLOR_PALETTE = [...]`).
- `zero_client/style.css`:
  - Streamlined `.message-header` and `.message-bubble` layout.
  - Dynamic peer color accent classes (e.g. `.peer-color-0`, `.peer-color-1`, etc.).
