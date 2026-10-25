Architectural Handover Brief: Unified Mobile & Desktop Connectivity Resilience (ticket-045)
1. Executive Summary & Objective
While recent updates (ticket-032 through ticket-044) addressed initial multi-peer ID alignment and mobile viewport/keyboard layout, live testing across mobile browsers (and multi-tab mobile sessions) exposed three critical connectivity and state-machine defects:

Premature Departure & Post-Disconnect Ghost Messaging on Background Tab Suspension: When a mobile (or throttled desktop) tab sits in the background past ~30–60 seconds, the OS suspends SCTP heartbeats, causing the active peer to fire handlePeerDeparture(peerId) ("Peer left chat" + message purge + adding peerId to departedPeers). When the user switches back to the suspended tab, the transport wakes up or mesh reconciliation reconnects the peers—allowing new messages to be delivered underneath "Peer left chat" while departedPeers permanently breaks the peer counter.
Role-Swap System Notice Bug ("Joined chat" vs "Peer joined chat"): Because the client relies on a single mutable global isHost boolean that gets mutated during limbo transitions or background reconnects, switching between tabs during the invite flow causes the inviter (Client 1) to falsely display "Joined chat" and the joiner (Client 2) to display "Peer joined chat".
Mobile Cellular (4G/5G) & Hairpinning Connection Failures: Non-trickle ICE gathering currently uses a blind timer without checking whether any public (srflx or relay) candidates have been gathered, and lacks TURN relay support for Carrier-Grade Symmetric NAT (CGNAT) and same-device cellular hairpinning.
Core Architectural Principle: Unified Adaptive Resilience
Do NOT fork the codebase into separate "mobile" and "desktop" client paths. Every resilience mechanism specified in ticket-045 applies uniformly to all client instances, strengthening both mobile browsers (iOS Safari, Android Chrome) and desktop browsers (background tab throttling, laptop sleep/wake, and restrictive firewalls).


2. Target File Scope
zero_client/index.html (Per-peer role tracking, transient disconnect grace window, departedPeers recovery cleanup, smart ICE candidate gathering, and optional TURN configuration)
zero_network/src/handlers.rs & zero_network/src/lib.rs (Optional/If configuring ephemeral Cloudflare TURN credentials endpoint, otherwise scoped strictly to zero_client/index.html)


3. Root-Cause Diagnosis & Required Engineering Fixes
Fix 1: Per-Peer Role Tracking (Eliminates "Joined chat" / "Peer joined chat" Reversal)
Root Cause: index.html uses a single global boolean isHost to decide whether onStateChangeCb("open", peerId) emits "Joined chat" or "Peer joined chat". During mobile app/tab switching, background socket closures, limbo state resets, or mesh reconciliation handshakes, isHost gets overwritten or evaluated out of context.
Required Implementation (zero_client/index.html):
Replace reliance on the global isHost flag for join notices with an explicit per-peer role map:

const peerRoles = new Map(); // peerId -> "inviter" | "joiner" | "mesh"

Record Roles Deterministically:
In createRoomAsHost() and handleHeaderAddPeerClick(): set peerRoles.set(assignedPeerId, "inviter") before creating the offer.
In joinRoomFromHash(): set peerRoles.set(hostPeerId, "joiner") before creating the answer.
In PeerJoined / RelaySdpOffer mesh introductions: if !peerRoles.has(remotePeerId), set peerRoles.set(remotePeerId, "mesh").
Emit Deterministic System Notice on First Open:
When peerId transitions to "open" for the first time (!announcedJoinPeers.has(peerId)):
Record announcedJoinPeers.add(peerId).
If peerRoles.get(peerId) === "joiner" and this is the initial room entry for the local client, display "Joined chat".
For all "inviter" and "mesh" peers (or any subsequent peer openings), display "Peer joined chat".
Guard Initial Signaling Against Premature Mesh Reconciliation:
Do not trigger reconcile_mesh or glare replacement offers for a peerId that is actively awaiting its initial hostWs or pendingInviteWs dead-drop answer.


Fix 2: Grace Window for Transient Background Freezes vs. Explicit Leave (PeerLeftAndBurn)
Root Cause: Currently, any unannounced WebRTC channel drop (dc.onclose or iceConnectionState === "disconnected" / "failed") immediately invokes handlePeerDeparture(peerId). On mobile browsers, backgrounding a tab for 30+ seconds suspends network heartbeats without firing beforeunload. When the user returns to the tab, the connection recovers or re-handshakes, but handlePeerDeparture has already printed "Peer left chat", purged messages, and added peerId to departedPeers (which permanently excludes peerId from get_connected_count()).
Required Implementation (zero_client/index.html):
Immediate Departure ONLY on Explicit PeerLeftAndBurn:
When an explicit PeerLeftAndBurn frame arrives over the DataChannel, cancel any pending grace timer for leavingPeerId, call handlePeerDeparture(leavingPeerId) immediately (purging their messages and emitting "Peer left chat" once), and call engine._teardownPeerQuietly(leavingPeerId).
Transient Disconnect Grace Timer (DISCONNECT_GRACE_MS = 12000) for Unannounced Drops:
Maintain const pendingDisconnectTimers = new Map(); // peerId -> timeoutId.
When onPeerStateChangeCb(peerId, "closed") or "disconnected" fires without having received PeerLeftAndBurn (and !departedPeers.has(peerId)):
Do not call handlePeerDeparture(peerId) immediately.
Start a 12,000ms (12-second) grace timer in pendingDisconnectTimers (and if the tab is currently hidden via document.visibilityState === "hidden", pause/extend the timer until visibilityState === "visible" + 8 seconds).
While in the grace window, trigger a non-destructive mesh reconciliation / reconnection attempt when visibilitychange transitions to "visible".
If the peer reconnects (state === "open") before the grace timer expires:
Clear and delete the timer from pendingDisconnectTimers.
Ensure departedPeers.delete(peerId) is called.
Do not emit "Peer left chat" or "Peer joined chat" again (since announcedJoinPeers.has(peerId) is already true), and keep all existing chat messages intact!
If the grace timer expires without recovery:
Delete the timer and execute handlePeerDeparture(peerId).
Self-Healing departedPeers Invariant:
If a peer ever transitions to "open" (even after a late reconnect), always execute departedPeers.delete(peerId) and update the header peer counter so the badge count never desynchronizes from the actual open DataChannels.


Fix 3: Smart Non-Trickle ICE Gathering & Cellular / CGNAT Support
Root Cause:
_waitForIceGatheringComplete(pc) currently waits up to a flat timeout (3000ms–4500ms) or exits without verifying if a routable public candidate (typ srflx or typ relay) was actually gathered.
Over 4G/5G cellular networks (or two tabs on the same phone over cellular data), Carrier-Grade Symmetric NAT (CGNAT) and cellular hairpinning blocks prevent pure STUN (srflx) from establishing direct UDP paths without either IPv6 or a TURN relay candidate (typ relay).
Required Implementation (zero_client/index.html):
Smart Candidate Inspection in _waitForIceGatheringComplete(pc):
Track gathered candidates in onicecandidate:
Detect when at least one srflx or relay candidate is present (ev.candidate.candidate.includes("typ srflx") || ev.candidate.candidate.includes("typ relay")).
Once at least one public/relay candidate is gathered, start a short 400ms settle timer to capture any secondary interface candidates, then resolve immediately (cutting desktop and Wi-Fi offer generation down from 3–4.5s to <600ms!).
If no srflx/relay candidate has arrived yet, wait up to 5000ms before resolving.
Expanded Multi-Port STUN & Configurable TURN Support (this.rtcConfig):
Include redundant STUN servers across standard ports (3478 and 19302) and support injecting TURN server configurations (window.ZERO_TURN_SERVERS or default public/Cloudflare TURN relay config) into this.rtcConfig.iceServers so devices behind Symmetric 4G/5G CGNAT or same-phone cellular hairpinning can fall back to encrypted typ relay DataChannel transport automatically.


4. Verification & Acceptance Criteria
Gate 1: Workspace Unit & Integration Suite
cargo test --workspace — All Rust unit and integration tests across zero_protocol, zero_client, and zero_network must pass.
./scripts/deploy_dry_run.sh — Must build and stage zero_client/dist cleanly.
Gate 2: Automated Browser Resilience Scenarios (Playwright / Headless)
Add automated verification covering the three exact failure modes:

Scenario A (Per-Peer Role Notice Invariant): Simulate Client 1 creating an invite, backgrounding/switching visibility during the dead-drop, and Client 2 joining. Assert Client 1 displays "Peer joined chat" and Client 2 displays "Joined chat" (never reversed).
Scenario B (Transient Tab Freeze & Recovery Without False Departure): Establish an open session between Client 1 and Client 2, exchange a message, simulate a transient transport drop (disconnected -> open within the 12s grace window), and assert:
"Peer left chat" is never rendered.
Existing messages are not purged.
Subsequent messages render normally and the peer counter remains 1.
Scenario C (True Unannounced Disconnect After Grace Expiry): Simulate a peer disconnecting and remaining unreachable past the grace window; assert "Peer left chat" is rendered once, the peer's messages are purged, and the peer counter decrements.
Scenario D (Explicit PeerLeftAndBurn Immediate Purge): Verify clicking Leave still triggers immediate message purge and "Peer left chat" with zero grace-period delay.
