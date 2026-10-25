# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-045"
title: "Unified Mobile & Desktop Connectivity Resilience: Per-Peer Roles, Background Disconnect Grace Period & Smart ICE Gathering"
state: "IN_PROGRESS"
created_at: "2026-10-25T20:10:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-045/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

Following live testing across mobile browsers (iOS Safari, Android Chrome) and multi-tab/throttled desktop sessions, three critical connectivity and state-machine defects have been diagnosed in [`docs/HANDOVER_TICKET_045.md`](file:///Users/mattcourtney/Library/CloudStorage/GoogleDrive-mattadam.courtney@gmail.com/My%20Drive/zero_chat/docs/HANDOVER_TICKET_045.md):

1. **Premature Departure & Post-Disconnect Ghost Messaging on Background Tab Suspension**:
   - When a mobile browser tab (or throttled desktop tab) is backgrounded for ~30–60 seconds, the OS suspends SCTP network heartbeats without firing `beforeunload`.
   - The active remote peer's WebRTC engine detects `dc.onclose` or `iceConnectionState === "disconnected"` and immediately executes `handlePeerDeparture(peerId)`: it prints `"Peer left chat"`, purges all messages authored by `peerId`, and adds `peerId` to `departedPeers`.
   - When the user switches back to the suspended tab, the WebRTC transport wakes up or background mesh reconciliation reconnects the peers (`dc.onopen`). Because the connection recovers, new chat messages can be received and displayed underneath `"Peer left chat"`. However, because `peerId` remains stuck in `departedPeers`, `get_connected_count()` permanently excludes it, breaking the header peer counter (`#peer-count` stuck at `0`).
2. **Role-Swap System Notice Bug ("Joined chat" vs "Peer joined chat")**:
   - In [`zero_client/index.html`](file:///Users/mattcourtney/Library/CloudStorage/GoogleDrive-mattadam.courtney@gmail.com/My%20Drive/zero_chat/zero_client/index.html), the client relies on a single global boolean `isHost` to decide whether `onStateChangeCb("open", peerId)` emits `"Joined chat"` or `"Peer joined chat"`.
   - During mobile app/tab switching, background socket closures, limbo state resets, or mesh reconciliation handshakes, `isHost` gets overwritten or evaluated out of context. Consequently, Client 1 (the room creator/inviter) falsely displays `"Joined chat"` and Client 2 (the joiner) displays `"Peer joined chat"`.
3. **Mobile Cellular (4G/5G), Hairpinning & Non-Trickle ICE Gathering Latency**:
   - `_waitForIceGatheringComplete(pc)` currently waits for a flat timer (3000ms–4500ms) or exits without verifying whether routable public candidates (`typ srflx` or `typ relay`) were gathered.
   - Over 4G/5G cellular networks or when testing two tabs on the same mobile device over cellular data, Carrier-Grade Symmetric NAT (CGNAT) and cellular hairpinning blocks prevent pure STUN from finding a direct UDP path without redundant STUN ports or TURN relay fallback.

### Core Architectural Principle: Unified Adaptive Resilience
This implementation maintains a single, unified codebase across all platforms. Every resilience mechanism applies uniformly to all client instances, fortifying both mobile browsers (iOS Safari, Android Chrome) and desktop environments (throttled background tabs, laptop sleep/wake, restrictive corporate NATs).

---

## 2. Requirements & Acceptance Criteria

### REQ-045.1: Deterministic Per-Peer Role Tracking & System Notice Invariant
- In [`zero_client/index.html`](file:///Users/mattcourtney/Library/CloudStorage/GoogleDrive-mattadam.courtney@gmail.com/My%20Drive/zero_chat/zero_client/index.html):
  - Replace the global `isHost` evaluation for join notices with an explicit per-peer role map:
    ```javascript
    const peerRoles = new Map(); // peerId -> "inviter" | "joiner" | "mesh"
    const announcedJoinPeers = new Set(); // peerId set of already announced joins
    let hasAnnouncedInitialSelfJoin = false;
    ```
  - **Deterministic Role Assignment**:
    - In `createRoomAsHost()` and `handleInRoomInvite()`: execute `peerRoles.set(assignedPeerId, "inviter")` prior to creating the offer.
    - In `joinRoomFromHash()`: execute `peerRoles.set(hostPeerId, "joiner")` prior to creating the answer.
    - In `PeerJoined` and `RelaySdpOffer` mesh introductions: if `!peerRoles.has(remotePeerId)`, execute `peerRoles.set(remotePeerId, "mesh")`.
  - **Deterministic Join Notice on State Transition to "open"**:
    - When `peerId` transitions to `"open"` for the first time (`!announcedJoinPeers.has(peerId)`):
      - Add `peerId` to `announcedJoinPeers`.
      - If `peerRoles.get(peerId) === "joiner"` AND `!hasAnnouncedInitialSelfJoin`:
        - Set `hasAnnouncedInitialSelfJoin = true`.
        - Emit `"Joined chat"` system message.
      - If `peerRoles.get(peerId) === "inviter"` or `"mesh"` (or for any subsequent peer channel connections):
        - Emit `"Peer joined chat"` system message.
  - **Signaling Guard Against Premature Glare / Reconciliation**:
    - Do not trigger `reconcile_mesh` or glare replacement offers for any `peerId` actively awaiting its initial `hostWs` or `pendingInviteWs` dead-drop answer.
  - **Reset Cleanup**:
    - In `resetToHome()`: execute `peerRoles.clear()`, `announcedJoinPeers.clear()`, and `hasAnnouncedInitialSelfJoin = false`.

### REQ-045.2: Transient Background Freeze Grace Period & Self-Healing `departedPeers`
- In [`zero_client/index.html`](file:///Users/mattcourtney/Library/CloudStorage/GoogleDrive-mattadam.courtney@gmail.com/My%20Drive/zero_chat/zero_client/index.html):
  - **Immediate Departure on Explicit `PeerLeftAndBurn`**:
    - When an explicit `PeerLeftAndBurn` frame is received over an open DataChannel:
      - Cancel and delete any pending disconnect timer for `leavingPeerId` in `pendingDisconnectTimers`.
      - Immediately invoke `handlePeerDeparture(leavingPeerId)` (purging messages and emitting `"Peer left chat"` once).
      - Execute `engine._teardownPeerQuietly(leavingPeerId)`.
  - **Transient Disconnect Grace Period (`DISCONNECT_GRACE_MS = 12000`) for Unannounced Channel Drops**:
    - Maintain `pendingDisconnectTimers = new Map(); // peerId -> { timeoutId, expiryTimestamp }`.
    - When `onPeerStateChangeCb(peerId, "closed")` or `"disconnected"` fires without having received an explicit `PeerLeftAndBurn` (and `!departedPeers.has(peerId)`):
      - Do NOT invoke `handlePeerDeparture(peerId)` immediately.
      - Calculate grace timeout: if `document.visibilityState === "hidden"`, record extended grace window (active until `visibilityState === "visible"` + 8000ms); otherwise 12000ms.
      - Schedule timer in `pendingDisconnectTimers`.
  - **Visibilitychange Recovery**:
    - On `document.addEventListener("visibilitychange")` when `visibilityState === "visible"`:
      - If any `peerId` has an active entry in `pendingDisconnectTimers`, extend/adjust its timer to now + 8000ms.
      - Trigger non-destructive mesh reconciliation to re-probe or re-establish pending peer channels.
  - **Channel Reconnection Before Grace Expiry**:
    - If `peerId` transitions to `"open"` while in `pendingDisconnectTimers`:
      - Clear and delete the timer from `pendingDisconnectTimers`.
      - Execute `departedPeers.delete(peerId)`.
      - Because `announcedJoinPeers.has(peerId)` is already true, do NOT emit duplicate `"Peer joined chat"` or `"Joined chat"` notices.
      - All existing chat messages and peer identity attribution remain intact in the DOM.
      - Update the header peer counter (`updatePeerCountDisplay()`).
  - **Grace Expiry Without Recovery**:
    - If the grace timer fires without channel recovery:
      - Delete `peerId` from `pendingDisconnectTimers`.
      - Execute `handlePeerDeparture(peerId)` (emitting `"Peer left chat"` and purging the peer's messages).
  - **Self-Healing `departedPeers` Invariant**:
    - Whenever any peer transitions to `"open"` (including late reconnection after a grace window expiration), always execute `departedPeers.delete(peerId)` and call `updatePeerCountDisplay()`, ensuring that active DataChannels are never locked out of the peer counter.

### REQ-045.3: Smart Non-Trickle ICE Gathering & Cellular / CGNAT Relay Support
- In [`zero_client/index.html`](file:///Users/mattcourtney/Library/CloudStorage/GoogleDrive-mattadam.courtney@gmail.com/My%20Drive/zero_chat/zero_client/index.html):
  - **Smart Candidate Inspection in `_waitForIceGatheringComplete(pc)`**:
    - Monitor candidates in `pc.onicecandidate`:
    - Detect when at least one public candidate (`typ srflx`) or relay candidate (`typ relay`) has been gathered:
      ```javascript
      const isPublicOrRelay = ev.candidate.candidate.includes("typ srflx") || ev.candidate.candidate.includes("typ relay");
      ```
    - Once detected, start a short 400ms settle timer to gather any secondary local interface candidates, then resolve immediately.
    - If no public/relay candidate has arrived, wait up to a maximum safety ceiling of 5000ms before resolving.
  - **Redundant Multi-Port STUN & Configurable TURN in `this.rtcConfig`**:
    - Configure redundant Google STUN servers on standard ports (19302 and 3478):
      - `stun:stun.l.google.com:19302`
      - `stun:stun1.l.google.com:19302`
      - `stun:stun2.l.google.com:19302`
      - `stun:stun.l.google.com:3478`
    - Check for `window.ZERO_TURN_SERVERS` and merge any defined TURN relay servers into `this.rtcConfig.iceServers`, allowing automatic fallback to encrypted `typ relay` transport across Symmetric 4G/5G CGNAT or same-phone cellular hairpinning.

### REQ-045.4: Automated Test & Verification Gates
- In [`zero_client/tests/browser/test_mesh_scenario.js`](file:///Users/mattcourtney/Library/CloudStorage/GoogleDrive-mattadam.courtney@gmail.com/My%20Drive/zero_chat/zero_client/tests/browser/test_mesh_scenario.js):
  - Implement **Test 21: Unified Mobile & Desktop Connectivity Resilience Suite**:
    - **Scenario A (Per-Peer Role Notice Invariant)**: Simulate Client 1 creating an invite, backgrounding/switching visibility during the dead-drop, and Client 2 joining. Assert Client 1 displays `"Peer joined chat"` and Client 2 displays `"Joined chat"` (never reversed).
    - **Scenario B (Transient Tab Freeze & Recovery Without False Departure)**: Establish an open session between Client 1 and Client 2, exchange a message, simulate a transient transport drop (disconnected -> open within 12s grace window), and assert:
      - `"Peer left chat"` is never rendered.
      - Existing messages are not purged.
      - Subsequent messages render normally and the peer counter remains 1.
    - **Scenario C (True Unannounced Disconnect After Grace Expiry)**: Simulate a peer disconnecting and remaining unreachable past the grace window; assert `"Peer left chat"` is rendered once, the peer's messages are purged, and the peer counter decrements to 0.
    - **Scenario D (Explicit `PeerLeftAndBurn` Immediate Purge)**: Verify clicking Leave still triggers immediate message purge and `"Peer left chat"` with zero grace-period delay.
    - **Scenario E (Smart ICE Gathering)**: Verify `_waitForIceGatheringComplete` resolves quickly upon detecting `typ srflx` candidate.
- All workspace Rust tests pass (`cargo test --workspace` -> 123 tests).
- Dual WASM compilation checks pass (`zero_client` and `zero_network`).
- Code formatting and linter checks pass (`cargo fmt -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`).
- All 21 browser tests pass (`bash zero_client/tests/browser/run_test.sh`).

---

## 3. Target Architecture & Affected Modules

| Module / File | Scope of Changes |
|---|---|
| [`zero_client/index.html`](file:///Users/mattcourtney/Library/CloudStorage/GoogleDrive-mattadam.courtney@gmail.com/My%20Drive/zero_chat/zero_client/index.html) | Implement `peerRoles` Map, `announcedJoinPeers` Set, `pendingDisconnectTimers` Map (12s grace window), visibilitychange recovery extension, smart ICE candidate inspection (400ms settle after `srflx`/`relay`), redundant STUN ports & configurable TURN support in `this.rtcConfig`. |
| [`zero_client/tests/browser/test_mesh_scenario.js`](file:///Users/mattcourtney/Library/CloudStorage/GoogleDrive-mattadam.courtney@gmail.com/My%20Drive/zero_chat/zero_client/tests/browser/test_mesh_scenario.js) | Add Test 21 covering Scenarios A, B, C, D, and E. |
| [`openspec/specs/overview.md`](file:///Users/mattcourtney/Library/CloudStorage/GoogleDrive-mattadam.courtney@gmail.com/My%20Drive/zero_chat/openspec/specs/overview.md) | Update canonical specification upon task completion (v1.19.0, Invariant 21). |

---

## 4. Verification & Quality Gates

1. **Gate 1: Rust Unit & Integration Tests**: `cargo test --workspace` (all 123 tests pass).
2. **Gate 2: Dual WASM Compilation**:
   - `cargo check --target wasm32-unknown-unknown -p zero_client`
   - `cargo check --target wasm32-unknown-unknown -p zero_network`
3. **Gate 3: Formatting & Linting**:
   - `cargo fmt -- --check`
   - `cargo clippy --workspace --all-targets -- -D warnings`
4. **Gate 4: Headless Browser Regression Suite**:
   - `bash zero_client/tests/browser/run_test.sh` (all 21 tests pass).
5. **Gate 5: Architectural & Security Review**: Independent Reviewer evaluation with `APPROVE` verdict.
