# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-025"
title: "Multi-Peer Browser WebRTC Engine & Connection Pool"
state: "PLANNING"
roadmap: "multi-user-mesh-chat"
depends_on: []
created_at: "2026-10-18T21:02:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-025/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/multi-user-mesh-chat.md`

## 1. Problem Statement & Objective
Currently, `zero_client/index.html` manages a single `RTCPeerConnection` instance and a single `RTCDataChannel` inside `ZeroWebRtcEngine`. As a result, the application can only communicate with 1 peer at a time.

This task refactors `ZeroWebRtcEngine` into a multi-peer WebRTC connection pool:
1. Maintain a dynamic map of connected peers: `Map<peerId, { pc: RTCPeerConnection, dc: RTCDataChannel, state: string }>`.
2. Allow creating multiple independent offers and accepting multiple independent answers concurrently.
3. Broadcast messages (`send_text(text)`) across all active, open DataChannels.
4. Support dynamic peer addition, individual peer state change monitoring, and individual peer teardown without terminating the overall room session.

---

## 2. Scope Boundaries
- **In-Scope**:
  - `zero_client/index.html`:
    - Refactor `ZeroWebRtcEngine` class to support multi-peer connection pooling.
    - Provide APIs: `create_peer_offer(peerId)`, `accept_peer_answer(peerId, answerSdp)`, `create_peer_answer(peerId, offerSdp)`, `broadcast_text(text)`, `close_peer(peerId)`.
    - Handle incoming DataChannels on `pc.ondatachannel` associated with their respective remote peers.
- **Out-of-Scope**:
  - In-band relay protocol handling between secondary peers (deferred to `ticket-026`).
  - Header Add-Peer UI button and clipboard lifecycle (deferred to `ticket-027`).

---

## 3. Requirements & Acceptance Criteria
- [ ] **REQ-025.1 (Multi-Peer Pool Architecture)**:
  - `ZeroWebRtcEngine` manages multiple `RTCPeerConnection` and `RTCDataChannel` instances indexed by unique `peerId`.
- [ ] **REQ-025.2 (Message Broadcasting)**:
  - `broadcast_text(text)` sends the payload to all currently open DataChannels in the pool.
- [ ] **REQ-025.3 (Individual Peer Lifecycle Tracking)**:
  - Individual peer disconnection transitions update only that peer's state; the engine remains active as long as at least one peer or active session exists.
- [ ] **REQ-025.4 (Workspace Integrity & Compilation)**:
  - Clean HTML/JS syntax and 0 regressions across existing test suite (`cargo test --workspace`).
