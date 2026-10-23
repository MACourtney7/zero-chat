# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-031"
title: "Multi-User Chat Resilient Full-Mesh Joining/Leaving"
state: "IN_PROGRESS"
created_at: "2026-10-23T21:20:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-031/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & Objective

**Observed (user report)**:
1. Client A creates a chat and invite; Client B joins. A generates a second invite; Client C joins.
2. A leaves. C sends a message. **B never displays it.**
3. Conclusion drawn: B and C are only connected *indirectly through A* (star topology), not directly.

**Expected**: a multi-user chat keeps working as long as **at least 2 clients remain connected**, regardless of who invited whom or who leaves first. Every client must hold a direct DataChannel to every other client (full mesh) so no participant is a single point of failure.

### Hypothesised Root Causes (to be confirmed by Reproduce step, REQ-031.1)
All in the browser mesh logic in `zero_client/index.html`; ticket-029/030 attempted fixes but the star topology persists:

| # | Hypothesis | Location |
|---|---|---|
| H1 | Secondary direct snap (`PeerJoined` -> `RelaySdpOffer` -> `RelaySdpAnswer`) silently fails or races (introduction sent before the new channel is usable, relay handshake aborted when the relay peer leaves mid-handshake, 3s ICE gathering timeout producing candidate-less SDP). | `engine.onPeerStateChangeCb`, `initiate_relay_handshake`, `_waitForIceGatheringComplete` |
| H2 | `create_peer_offer` / `create_peer_answer` call `close_peer()` on an existing entry, firing `onPeerStateChangeCb("closed")` -> spurious `handlePeerDeparture` (adds to `departedPeers`, purges bubbles, emits "Peer left chat") and may tear down a live link. `departedPeers` is never cleared on reconnect. | `create_peer_offer`, `create_peer_answer`, `handlePeerDeparture` |
| H3 | `activeSessionPeers` is populated optimistically (on `PeerJoined`, `RelaySdpOffer/Answer`) before any DataChannel is open, so the counter and `survivingCount = max(openPeerCount, activeSessionPeers.size)` report a healthy session while no direct channel exists, masking failure. | `updatePeerCountDisplay`, `handlePeerDeparture` |
| H4 | Only one side initiates (`myPeerId < remotePeerId`); if that offer is lost, the other never retries - no retry, timeout, or reconciliation of expected vs. actual mesh links. | `PeerJoined` handler |
| H5 | Send path (`broadcast_text`) throws or sends only to existing channels; sender shows message locally with no failure indication. | `broadcast_text`, send handler |

## 2. Scope Boundaries
- **In-Scope**:
  - `zero_client/index.html`: mesh engine, introduction/relay-handshake logic, peer lifecycle, departure handling, peer counter source of truth.
  - `zero_client/src/mesh.rs` / `zero_client/src/ui.rs`: Rust-side parity of protocol/state-machine changes, and tests.
  - `tests/` and `zero_client/tests/`: simulation/regression tests for the A/B/C scenario.
- **Out-of-Scope**:
  - Cloudflare Worker / Durable Object signalling (`zero_network`).
  - TURN servers / guaranteed NAT traversal (a direct-connection failure between two NATed peers is reported, not solved).
  - UI redesign.

## 3. Requirements & Acceptance Criteria

- [ ] **REQ-031.1 (Reproduce & Diagnose First)**:
  - A failing automated test (Rust mesh simulation and/or headless browser scenario) reproduces: A-B joined, A-C joined, A leaves, C sends, B receives nothing.
  - The implementer documents the confirmed root cause(s) (H1-H5 or new) in its report before fixing.
- [ ] **REQ-031.2 (Full-Mesh Guarantee)**:
  - When C joins via any member, within a bounded time (<= 10s) C holds a direct open DataChannel to **every** other member, verified on both ends.
  - Joins through a different inviter (e.g. B invites D) likewise yield links to all members.
  - Introduction/handshake is idempotent, glare-safe, and never closes/replaces an already-open channel (H2).
- [ ] **REQ-031.3 (Mesh Reconciliation / Retry)**:
  - Each client tracks known session members (from `PeerJoined` introductions) and periodically reconciles them against open channels; a missing link triggers a re-handshake with bounded retries/backoff via any still-connected peer as relay.
  - Handshake failure after retries is logged and surfaced, not silent.
  - Either side may initiate after the deterministic tie-breaker window expires (H4).
- [ ] **REQ-031.4 (Survives Any Departure While >= 2 Remain)**:
  - After the original host (or any single member) leaves, remaining clients keep direct channels, can send/receive to each other, and the chat stays active.
  - Only the departed peer's bubbles are purged; others' messages preserved.
  - The session returns to home only when no other peer remains.
- [ ] **REQ-031.5 (Truthful Peer Count & State)**:
  - `#peer-count` and `survivingCount` derive from actually **open** DataChannels only (H3).
  - Re-handshaking a link never triggers "Peer left chat" or purges; `departedPeers` entries are cleared when that peer legitimately reconnects.
- [ ] **REQ-031.6 (Send Reliability Feedback)**:
  - Messages go to all open channels; if none are open the UI shows a clear error instead of showing the message as delivered (H5).
  - Gossip forwarding stays as fallback for not-yet-direct links, with dedup preserved.
- [ ] **REQ-031.7 (Workspace Integrity)**:
  - `cargo test --workspace` passes, `cargo clippy --workspace --all-targets -- -D warnings` clean, `wasm32-unknown-unknown` build clean; ticket-029/030 behaviours (single-click copy invite, system messages, selective purge) not regressed.

---

## 4. Verification Plan

| Test ID | Description | Expected Result |
|---|---|---|
| `V-031-1` | Sim: A-B, A invites C; assert link table | B-C direct link exists; all three have 2 open peers |
| `V-031-2` | After V-031-1, A leaves; C sends | B receives; both stay active, count 1 each |
| `V-031-3` | After V-031-1, B leaves instead; A sends | C receives; symmetric |
| `V-031-4` | 4 clients, chain joins (A->B, B->C, C->D), A leaves | B, C, D fully meshed and chatting |
| `V-031-5` | Relay handshake dropped once | Reconciliation retries; link established |
| `V-031-6` | Re-handshake over open link | Channel not closed; no "Peer left chat"; no purge |
| `V-031-7` | Last two clients, one leaves | Remaining client returns to home state |
| `V-031-8` | Browser (headless/manual): 3 tabs per user scenario | B displays C's message after A closes tab |
| `V-031-9` | Workspace test/clippy/wasm build | All pass |

## 5. Target Architecture & Affected Modules
- `zero_client/index.html` (JS mesh engine + UI glue) - primary.
- `zero_client/src/mesh.rs`, `zero_client/src/ui.rs` - parity + tests.
- `tests/mesh_simulation_test.rs`, `zero_client/tests/*` - regression tests.

## 6. Brief Decomposition (executed by `implement-task`)

Stage 1 (parallel, disjoint blast radius):

| Brief | Size | Files | Covers |
|---|---|---|---|
| `brief-impl-031-browser` - diagnose (REQ-031.1) and fix JS mesh H1-H5, truthful count, reconciliation loop | H | `zero_client/index.html` | REQ-031.1-.6 (browser) |
| `brief-impl-031-rust` - Rust mesh: A/B/C regression sim tests V-031-1..7, reconciliation + idempotent snap parity | H | `zero_client/src/mesh.rs`, `zero_client/tests/mesh_simulation_test.rs` | REQ-031.1-.5 (Rust model) |

Stage 2: `brief-validation` (L), then reviewer cycles (max 3).

## 7. Risks / Open Questions
- Browser root cause may only reproduce with real WebRTC; mitigation: V-031-8.
- Symmetric NAT without TURN can legitimately block B-C; we surface the failure and keep the star link as gossip fallback.
