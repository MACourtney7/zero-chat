# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-026"
title: "In-Band SDP Signaling Relay & Mesh Snapping in Browser Client"
state: "IN_PROGRESS"
roadmap: "multi-user-mesh-chat"
depends_on: ["ticket-025"]
created_at: "2026-10-18T21:02:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-026/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/multi-user-mesh-chat.md`

## 1. Problem Statement & Objective
When multiple peers join a room hosted by Alice (e.g. Bob and Charlie), establishing direct P2P connections between Bob and Charlie without contacting Cloudflare servers requires **in-band signaling relay** (REQ-006.1, REQ-006.2).

This task implements in-band SDP relay handling inside the browser client (`zero_client/index.html`):
1. When secondary peer Charlie connects to Alice, Alice introduces Charlie to Bob by transmitting a relay offer frame over existing DataChannels.
2. Bob and Charlie exchange SDP offer/answer through Alice's bridge.
3. Bob and Charlie snap a direct WebRTC DataChannel directly between each other with **0 cloud server requests**.
4. Gossip message forwarding and loop deduplication ensure messages reach all nodes once.

---

## 2. Scope Boundaries
- **In-Scope**:
  - `zero_client/index.html`:
    - Implement in-band relay signaling protocols matching `zero_protocol::envelope::MeshRelaySignal`.
    - Handle `RelaySdpOffer` and `RelaySdpAnswer` payload routing in `engine.onMessageCb`.
    - Direct channel snapping between secondary peers.
    - Message deduplication cache (`seen_message_ids`).
- **Out-of-Scope**:
  - Cloudflare worker edge changes (edge is severed after dead drop).
  - UI Header Add-Peer button (deferred to `ticket-027`).

---

## 3. Requirements & Acceptance Criteria
- [x] **REQ-026.1 (In-Band SDP Relay Bridging)**:
  - Host transparently forwards sealed relay signaling frames between connected peers without modifying payloads.
- [x] **REQ-026.2 (Direct Mesh Snapping)**:
  - Secondary peers snap direct WebRTC DataChannels with 0 edge requests.
- [x] **REQ-026.3 (Gossip Message Flooding & Loop Suppression)**:
  - Multi-hop chat messages flood across direct links with TTL hops and message deduplication.
- [x] **REQ-026.4 (Workspace Integrity & Regression)**:
  - Full workspace test suite passes with 0 failures and 0 warnings.
