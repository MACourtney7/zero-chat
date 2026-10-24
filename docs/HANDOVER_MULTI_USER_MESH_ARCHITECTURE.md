# Handover Brief: Multi-User WebRTC Full-Mesh Architecture & Reliability

**Date**: 2026-10-24  
**Target Audience**: Lead Architect & Implementing Agents for ZeroChat  
**System**: `zero_chat`  
**Current Milestone**: Multi-User P2P Mesh Chat  

---

## 1. Executive Summary & Core Objective

### What We Are Trying to Achieve
ZeroChat is designed as an **ephemeral, zero-footprint, serverless peer-to-peer (P2P) communication platform**. It uses pure Rust compiled to WebAssembly (`zero_client`), coupled with an ephemeral Cloudflare Worker / Hibernating Durable Object bootstrap (`zero_network`), to deliver encrypted browser-to-browser WebRTC DataChannels.

### Core Architectural Invariants
1. **Zero Forensic Trace**: No server-side persistence, no databases, no cookies, no `localStorage` or `sessionStorage`. Closing the tab permanently incinerates all keys and messages.
2. **Severed Cloud Edge**: The Cloudflare Durable Object acts *only* as an initial signaling dead-drop to exchange SDP offers and answers. Once direct WebRTC DataChannels open, the signaling socket closes permanently (`is_edge_severed == true`).
3. **Multi-User Resilient Full Mesh ($K_N$)**:
   - In any room with $N$ participants (e.g. 3 users: A, B, C), **every user must hold a direct, pairwise WebRTC DataChannel to every other user** ($N(N-1)/2$ total channels).
   - **Room Survivability**: As long as **any $\ge 2$ participants remain connected**, the chat room must remain alive and functional.
   - **Host Departure Invariance**: If Client 1 (the initial creator/host) leaves, Client 2 and Client 3 must stay connected, keep chatting, and continue to receive each other's messages seamlessly. The host is not a master server; all nodes are symmetric peers.

---

## 2. Problem Statement & Real-Browser Failure Symptoms

### Observed Behavior (Browser Manual Test)
1. **Initial 2-User Pair**: Client 1 creates chat; Client 2 joins via invite URL.
   - *Result*: Both clients display `#peer-count` badge = **1**. (Working as expected).
2. **3rd User Joins**: Client 1 generates a second invite URL (`#header-add-btn`); Client 3 joins via this URL.
   - *Result*:
     - Client 1 displays `#peer-count` = **2**.
     - Client 2 displays `#peer-count` = **1** (❌ **Defect 1**: Client 2 did not connect to Client 3).
     - Client 3 displays `#peer-count` = **1** (❌ **Defect 2**: Client 3 did not connect to Client 2).
3. **1st Client (Host) Departs**: Client 1 closes their tab or clicks Leave.
   - *Result*:
     - Client 2 and Client 3 both see their `#peer-count` drop to **0**.
     - Client 2 and Client 3 are completely disconnected and cannot send or receive messages.
     - On both Client 2 and Client 3, the system notification `"Peer left chat"` is printed **twice** (❌ **Defect 3**: Duplicate departure notices).
     - The chat session is dead for all remaining participants.

### Root Symptom
The network topology never established the direct $B \leftrightarrow C$ link. It operated purely as a **centralized star topology** ($B \leftrightarrow A \leftrightarrow C$). When Hub $A$ departed, the entire network collapsed into isolated islands.

---

## 3. Why Unit/Simulation Tests Passed While Real Browsers Failed

In previous sprints (Tickets 029, 030, 031), implementers wrote simulation tests in Rust (`tests/mesh_simulation_test.rs`) and headless JavaScript tests (`zero_client/tests/browser/test_mesh_scenario.js`).

**Why did these tests pass 100%?**
- **Mock WebRTC Does Not Simulate Real Networking**:
  In `mock_environment.js`, `RTCPeerConnection` and `RTCDataChannel` are mocked in memory. There is no STUN/TURN gathering, no UDP packet transmission, no NAT filtering, no real ICE state transitions, and candidate gathering finishes in 0 milliseconds.
- **In-Memory Assumptions**:
  In mocks, calling `dc.send(text)` immediately invokes `dc.onmessage(ev)` on the target synchronously. In real browsers, WebRTC DataChannels require real ICE negotiation over the network, asynchronous candidate gathering, and strict signaling state transitions (`stable`, `have-local-offer`, etc.).

---

## 4. In-Depth Root Cause Analysis of the Current Connection Design

### 4.1 How Pairwise Bootstrapping Works (Client 1 $\leftrightarrow$ Client 2, Client 1 $\leftrightarrow$ Client 3)
1. Client 1 creates an offer `create_peer_offer("peer-01")`.
2. Client 1 packs the SDP offer into URL fragment `#payload=<base64>` along with room secret HMAC token.
3. Client 1 opens a WebSocket to `WORKER_WS_ORIGIN/host/:do_id`.
4. Client 2 opens the URL, parses the offer, generates an answer via `create_peer_answer`, and `POST`s the answer to Cloudflare Worker dead-drop `/drop/:do_id/:uuid`.
5. Cloudflare Worker forwards answer to Client 1's WebSocket, burns the drop (410), and Client 1 applies the answer.
6. The DataChannel opens. The signaling WebSocket is closed. **This part works reliably.**

### 4.2 How Secondary Mesh Snapping Is Attempted ($B \leftrightarrow C$)
When Client 3 joins Client 1:
1. Client 1 detects the new open channel to Client 3.
2. In `engine.onPeerStateChangeCb("open")`, Client 1 loops over all existing peers and attempts "Universal Peer Introduction":
   - Sends `PeerJoined { peer_id: realExistingId }` to Client 3 over `Channel_13`.
   - Sends `PeerJoined { peer_id: realNewId }` to Client 2 over `Channel_12`.
3. Client 2 and Client 3 receive `PeerJoined`.
4. Tie-breaker rule: Node with `myPeerId < remotePeerId` initiates in-band relay handshake:
   - Calls `initiate_relay_handshake(relayPeerId, targetPeerId)`.
   - Creates a local SDP offer: `create_peer_offer(targetPeerId)`.
   - Sends `RelaySdpOffer { from_peer_id, to_peer_id, sealed_offer }` to Client 1 over the existing DataChannel.
5. Client 1 is supposed to receive `RelaySdpOffer`, see that `to_peer_id !== myPeerId`, and forward it to the other peer over its DataChannel.
6. The target peer receives `RelaySdpOffer`, creates `RelaySdpAnswer`, and sends it back through Client 1.
7. Both nodes apply the SDP and WebRTC connects directly.

### 4.3 Why Secondary Mesh Snapping Fails in Practice

#### Flaw 1: Peer ID Mismatch & Timing Race during Invite Creation
- When Client 1 generates the invite for Client 3 (`handleHeaderAddPeerClick`), it assigns a temporary random ID:
  ```javascript
  const invitePeerId = "peer-" + Math.random().toString(36).slice(2, 8);
  const offerSdp = await engine.create_peer_offer(invitePeerId);
  ```
- Client 1 stores the connection under `invitePeerId` (e.g. `peer-xyz`).
- When Client 3 joins, Client 3 sends its actual `localPeerId` (e.g. `peer-333333`) in `joiner_peer_id`.
- Client 1 receives the answer and attempts `engine.rename_peer(invitePeerId, drop.joiner_peer_id)`.
- **The Bug**:
  - If `ch.onopen` fires before or concurrently with `rename_peer`, `onPeerStateChangeCb` runs with the old or partially updated ID.
  - Client 1 introduces Client 2 to `invitePeerId` instead of `peer-333333`, or Client 2 addresses `RelaySdpOffer` to `peer-333333`, which Client 1 cannot route because `engine.peers` has not mapped it cleanly.
  - `engine.send_text(..., to_peer_id)` fails with `DataChannel for peer ... is not open` and the offer is silently dropped.

#### Flaw 2: Non-Trickle ICE Gathering Latency & Timeout
- `_waitForIceGatheringComplete` halts execution for up to 3000ms:
  ```javascript
  pc.addEventListener("icegatheringstatechange", check);
  timer = setTimeout(cleanup, 3000);
  ```
- In real browser environments, generating an offer or answer with STUN candidates often takes 500ms to 2500ms.
- While Node B is awaiting `_waitForIceGatheringComplete`, in-flight state is delicate. If any message is dropped or arrives out of sequence, the entire single-shot relay offer fails.

#### Flaw 3: Absence of a Formal Signaling State Machine
- The current implementation treats WebRTC negotiation as an ad-hoc set of fire-and-forget JSON frames (`PeerJoined`, `RelaySdpOffer`, `RelaySdpAnswer`).
- There are no acknowledgments (`ACK`), no ping/pong, and no protocol state machine (such as `IDLE`, `OFFERING`, `WAITING_ANSWER`, `CONNECTED`, `FAILED`).
- If an offer or answer frame is lost or delayed, the peers have no mechanism to re-synchronize state.

#### Flaw 4: Duplicate "Peer Left Chat" Notices
- In `onMessageCb`:
  ```javascript
  if (leavingPeerId && activeEng && activeEng.peers && activeEng.peers.has(leavingPeerId)) {
      activeEng.close_peer(leavingPeerId); // Calls onPeerStateChangeCb("closed") -> handlePeerDeparture(leavingPeerId)
  }
  handlePeerDeparture(leavingPeerId); // Called a second time here!
  ```
- `activeEng.close_peer()` triggers `onPeerStateChangeCb(peerId, "closed")`, which invokes `handlePeerDeparture(peerId)` (1st notice).
- Then, `onMessageCb` explicitly invokes `handlePeerDeparture(leavingPeerId)` again (2nd notice).

---

## 5. Architectural Critique & Proposed Solutions

We must stop treating this as an isolated bug fix and instead design a robust, verifiable connection architecture.

### Architectural Options

```
OPTION 1: Robust In-Band Relay Protocol (Pure Serverless / Edge Severed)
----------------------------------------------------------------------
Client 2 <====== (Direct WebRTC DataChannel) ======> Client 3
   ^                                                    ^
   |                                                    |
   +--- Relay SDP Offer/Answer/Candidates via Client 1 -+
        (Formal Handshake Protocol over existing DataChannels)

OPTION 2: Ephemeral Room Mesh Coordinator (Durable Object Aided)
----------------------------------------------------------------------
Client 1, Client 2, Client 3 use Cloudflare Durable Object room socket
STRICTLY for multi-party signaling during join/leave events, then hibernate.
```

### Evaluation of Options

#### Option 1: Robust In-Band Relay Protocol (Recommended for Pure P2P Invariant)
To preserve the core invariant that the cloud edge is completely severed and costs $0/second:
- Keep the in-band relay through existing peers, but replace the fragile ad-hoc JSON messages with a **formal WebRTC signaling protocol**:
  1. **Canonical Peer Identity**:
     - Peer IDs must be established deterministically. Never use temporary random `invitePeerId` placeholders that require renaming. The host offer can use a wildcard or placeholder session ID, but the peer ID MUST be resolved before any peer introduction is broadcast.
  2. **Formal Handshake State Machine**:
     - Implement a per-peer state machine:
       `DISCOVERED` $\rightarrow$ `OFFERING` $\rightarrow$ `ANSWERING` $\rightarrow$ `ESTABLISHED` $\rightarrow$ `FAILED`.
     - Implement request/response with sequence numbers and explicit `ACK`s.
  3. **Trickle ICE or Reliable Candidate Packaging**:
     - Ensure candidate gathering either reliably completes or candidates are trickled across the relay channel (`RelayIceCandidate`).
  4. **Connection Heartbeat / Verification**:
     - Do not declare two peers connected simply because `setRemoteDescription` was called. A direct DataChannel is ONLY open when `ch.onopen` fires and an initial P2P `PING`/`PONG` handshake completes.

#### Option 2: Multi-Party Signaling Room (Pragmatic Alternative)
- If in-band relay across NATs proves too fragile without a TURN server, the Cloudflare Durable Object room can remain as an on-demand signaling hub:
  - When Client 3 joins, the Durable Object broadcasts Client 3's offer to Client 2 directly, rather than relying on Client 1's browser to relay it.
  - *Trade-off*: Does this violate Invariant 2 ("Severed Cloud Edge")?
    - If the WebSocket closes once the mesh is formed, the edge is still severed during active chat. However, it requires keeping signaling infrastructure active whenever new peers join.

---

## 6. Actionable Checklist for the Implementing Agent

When undertaking the implementation:

1. **Test in Real Browsers (Mandatory)**:
   - Do NOT rely solely on mock unit tests. Open 3 browser tabs (`localhost:8080` or deployed URL):
     - Window A: Create room.
     - Window B: Join via link 1.
     - Window A: Generate link 2 (`+` button).
     - Window C: Join via link 2.
   - Inspect the DevTools console in all 3 windows:
     - Check `console.log` for `RelaySdpOffer`, `RelaySdpAnswer`, and `PeerJoined`.
     - Check `RTCPeerConnection.connectionState` and `iceConnectionState` on Window B and Window C.
2. **Fix Peer Identity & Renaming**:
   - Audit `handleHeaderAddPeerClick` and `ZeroWebRtcEngine.prototype.rename_peer`.
   - Ensure Client 1 never sends `PeerJoined` until the new peer's real ID is fully mapped in `this.peers`.
3. **Trace In-Band Relay Messages**:
   - Verify that when Client 2 sends `RelaySdpOffer` to Client 3 via Client 1, Client 1 actually receives it, routes it, and dispatches it out to Client 3's DataChannel without throwing `DataChannel not open`.
4. **Fix Duplicate Departure Calls**:
   - Clean up `onMessageCb` and `onPeerStateChangeCb("closed")` so `handlePeerDeparture` is invoked exactly once per leaving peer.
5. **Verify Full-Mesh Resilience**:
   - Close Window A (Client 1).
   - Window B and Window C must retain `#peer-count = 1`.
   - Send message from Window B $\rightarrow$ must appear in Window C.
   - Send message from Window C $\rightarrow$ must appear in Window B.
