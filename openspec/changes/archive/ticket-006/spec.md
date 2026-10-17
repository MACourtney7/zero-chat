# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-006"
title: "In-Band Gossip Signaling Relay & Multi-Peer Mesh Topology"
state: "PLANNING"
roadmap: "serverless-p2p-chat"
depends_on: ["ticket-005"]
created_at: "2026-10-16T21:28:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-006/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/serverless-p2p-chat.md`

## 1. Problem Statement & Objective
While the initial peer joins via a single dead-drop `POST` to the host (Alice), multi-peer scaling must not incur additional cloud edge requests. To enable arbitrary multi-participant chat rooms, Alice's browser acts as a local in-band signaling bridge, routing encrypted WebRTC SDP offers and answers between existing data channels (`Channel_AB`, `Channel_AC`). This allows Charlie and Bob to establish a direct P2P mesh connection (`Channel_BC`) with zero cloud signaling. The client dynamically maintains an N-peer mesh, broadcasting chat messages across direct links and handling peer disconnects gracefully.

## 2. Scope Boundaries
- **In-Scope**:
  - `zero_client/src/mesh.rs`: Multi-peer topology manager tracking connected peers (`PeerId` $\rightarrow$ `RTCDataChannel`).
  - In-band signaling relay protocol: Encapsulating secondary SDP offers/answers and ICE candidates in encrypted DataChannel frames routed via intermediate peers.
  - Mesh snapping algorithm: When Charlie connects to Alice, Alice informs Bob of Charlie's peer public key; Bob and Charlie exchange offers/answers via Alice to establish a direct `Channel_BC` data channel.
  - Broadcast flooding & deduplication: Message envelope includes a unique `Message_UUID` and TTL counter to prevent broadcast loops across the mesh.
  - Peer presence lifecycle: Handling peer disconnects and re-configuring active routing tables.
  - Multi-node simulation test verifying 3+ peer mesh formation without edge hits.
- **Out-of-Scope**:
  - TURN relay fallback (pure direct UDP DataChannel connectivity assumed).
  - Heavy distributed consensus algorithms (ephemeral gossip broadcast is sufficient).

## 3. Requirements & Acceptance Criteria
- [ ] **REQ-006.1**: Host bridges signaling messages between existing peers without inspecting or decrypting their end-to-end payloads.
  - *Verification*: Relay envelope forwards encrypted sub-payloads opaque to the bridge.
- [ ] **REQ-006.2**: Third participant (Charlie) establishes direct `Channel_BC` data channel with Bob via Alice's in-band bridge without any cloud edge requests.
  - *Verification*: Network log inspection confirms zero outbound HTTP/WebSocket requests during Charlie-Bob snap.
- [ ] **REQ-006.3**: Mesh broadcast delivers chat messages to all connected participants.
  - *Verification*: Simulation confirms a message sent by Bob appears on Alice's and Charlie's feeds.
- [ ] **REQ-006.4**: Duplicate message broadcast loops are detected and suppressed via message ID caches.
  - *Verification*: Flooding test asserts each message is displayed exactly once per peer.
- [ ] **REQ-006.5**: Disconnection of a peer is detected, tearing down associated channels and updating the UI peer list.
  - *Verification*: Channel close event cleans up peer entry from routing table and UI.

## 4. Architectural Design & Impacted Modules
- **New Modules / Files**:
  - `zero_client/src/mesh.rs`
  - `tests/mesh_simulation_test.rs`
- **Modified Modules**:
  - `zero_client/src/webrtc.rs`
  - `zero_client/src/ui.rs`
  - `zero_client/src/lib.rs`
  - `crates/zero_protocol/src/envelope.rs`

## 5. Decomposition Plan (Architect Briefs)

| Batch | Brief ID | Role | Target Thinking Level | Description | Files Scope | Dependencies |
| :---: | :--- | :--- | :---: | :--- | :--- | :--- |
| **1** | `brief-01-mesh-topology` | Implementer | `HIGH` | N-peer connection pool, in-band SDP relay routing, and direct channel snap | `zero_client/src/mesh.rs`, `zero_client/src/webrtc.rs` | None |
| **1** | `brief-02-mesh-broadcast-ui` | Implementer | `MEDIUM` | Deduplicating broadcast flooding, TTL enforcement, and multi-peer UI indicators | `zero_client/src/ui.rs`, `crates/zero_protocol/src/envelope.rs` | None |
| **2** | `brief-val-mesh-sim` | Validator | `HIGH` | 3+ peer in-band mesh simulation and loop prevention test | `tests/mesh_simulation_test.rs` | Batch 1 |
| **2** | `brief-val-lint` | Validator | `LOW` | Clippy and formatting audit across mesh implementation | Full repository | Batch 1 |
| **3** | `brief-rev` | Reviewer | `HIGH` | Distributed routing audit, deadlock prevention, and connection lifecycle review | Candidate diff | Batch 2 |
