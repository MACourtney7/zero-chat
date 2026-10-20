# Canonical System Architecture: zero_chat

```yaml
---
system: "zero_chat"
version: "1.4.0"
milestone: "multi-user-mesh-chat"
status: "PRODUCTION_CANONICAL"
updated_at: "2026-10-20T20:53:00Z"
---
```

## 1. Project Overview & Vision
`zero_chat` is an entirely serverless, zero-footprint, real-time peer-to-peer (P2P) communication platform operating inside web browsers using pure Rust compiled to WebAssembly (`wasm32-unknown-unknown`), coupled with an ephemeral Cloudflare Worker / Hibernating Durable Object signaling bootstrap utilizing WebSocket Hibernation, and automated dual-target CI/CD delivery.

### Core Architectural Invariants
1. **Zero Forensic Footprint**: No account registrations, no cookies, no `localStorage`, `sessionStorage`, or `indexedDB` persistence. Closing or reloading the browser tab immediately incinerates all in-memory keys, sessions, and chat feeds.
2. **Stateless HMAC-Sealed Handshake (Pattern B)**: Signaling dead-drop HTTP `POST` requests and hibernating WebSockets ($0 idle duration billing). Offline invites are sealed with `HMAC_SHA256(host_secret, Invite_UUID)` embedded in URL `#fragment`s, which never touch any cloud edge.
3. **Pure Rust Cryptographic Engine**: Synchronous, memory-safe cryptographic primitives (`x25519-dalek`, `chacha20poly1305`, `hmac-sha256` with `getrandom/js`) shared across WASM and Cloudflare Worker, bypassing asynchronous JS WebCrypto bridging overhead.
4. **Direct P2P DataChannels**: Once the initial WebRTC SDP offer/answer exchange completes, the cloud edge drops out permanently (`is_edge_severed() == true`). All communication flows directly over browser-to-browser UDP DataChannels.
5. **In-Band Gossip Mesh, Secondary Inviter Snapping & Glare Avoidance**: Multi-peer scaling without cloud edge requests. Any connected participant (host or invitee) can invite additional peers. The inviter serves as an opaque in-band relay bridge for secondary SDP exchange (`RelaySdpOffer`, `RelaySdpAnswer`), ensuring full triangle ($K_3$) or complete mesh snapping. Glare collisions are avoided via deterministic lexicographical tie-breaking (`myPeerId < remotePeerId`), with complete candidate gathering guarantees (`_waitForIceGatheringComplete`).
6. **Mesh-Coordinated Leave vs. Burn Lifecycle**:
   - **Leave Chat (`leave_and_burn`)**: Clean departures broadcast `PeerLeftAndBurn` across the mesh before severance, causing remaining peers to selectively purge all messages authored by the leaving peer (`data-peer-id`) while preserving active mesh connections; the departing user's local session is reset to home. Surviving peers maintain pairwise DataChannels and feed continuity even if the original host leaves.
   - **Burn Chat (`global_burn`)**: Triggers complete room annihilation (`GlobalBurnSession`) flooded across all peers, immediately closing DataChannels, zeroizing keys/memory, and returning all participants simultaneously to the home view.
   - **Dirty Disconnect Purge**: Abrupt disconnections (tab close, network severance) are intercepted via WebRTC channel state monitors and unload events to automatically purge the departed peer's messages on surviving clients.
7. **Simplified Home View, Single-Invite Lifecycle & Peer Counter Synchronization**:
   - Decentralized multi-participant mesh rooms supported in pure browser client with connection pooling (`this.peers = new Map()`).
   - Frictionless room creation: clicking `#btn-create-room` in home view immediately copies the invite link to `navigator.clipboard` with clear visual feedback (`.btn-copied` state and system notice), completely eliminating `#invite-bar`.
   - In-room peer invites are generated via an Add-Peer button (`#header-add-btn`) placed in the header directly to the left of `#peer-count`.
   - Enforces a strictly non-wasteful single-invite lifecycle: only one invite can be active at a time; subsequent clicks while pending re-copy the existing valid link without spawning redundant cloud signaling sessions; cleared upon peer connection; copies silently without tooltip bubbles.
   - **Presence-Aware Dynamic Peer Counter Synchronization**: `#peer-count` evaluates `Math.max(engine ? engine.openPeerCount : 0, activeSessionPeers.size)`. Immediate presence tracking on `PeerJoined` and relay messages ensures all participants' counters update in lockstep to `2` upon 3rd-peer arrival without waiting on asynchronous ICE negotiation. Upon peer departure, counters decrement symmetrically (`2 -> 1`) across all surviving clients.
   - Standardized system messages: clean, user-friendly notices for room creation (`"Secure chat created"`), self joins (`"Joined chat"`), peer arrivals (`"Peer joined chat"` without debug string leakage), and peer departures (`"Peer left chat"`).
8. **Hermetic CI/CD Pipelines**: Automated dual-target test validation, strict clippy auditing, and automated Cloudflare Worker + Cloudflare Pages deployment via GitHub Actions.

---

## 2. Directory Layout & Subsystem Ownership

```text
zero_chat/
├── .agents/                        # Antigravity IDE configuration & plugins
│   ├── plugins.json                # Software factory plugin reference
│   └── mcp_config.json             # MCP server configuration
├── .factory/                       # Factory state machine runtime (gitignored)
├── .github/
│   └── workflows/                  # CI/CD automation pipelines
│       ├── ci.yml                  # Format, clippy, test, and wasm build validation
│       └── deploy.yml              # Cloudflare Worker and Pages deployment
├── crates/
│   └── zero_protocol/              # Shared schemas, DTOs, & crypto primitives
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── crypto.rs           # HMAC-SHA256, X25519, ChaCha20-Poly1305
│           └── envelope.rs         # Signaling, chat, and mesh relay envelopes
├── zero_client/                    # Pure Rust WASM client & WebRTC engine
│   ├── Cargo.toml
│   ├── index.html                  # Zero-npm HTML/CSS web frontend
│   ├── style.css                   # Glassmorphic dark-mode styling
│   └── src/
│       ├── lib.rs
│       ├── webrtc.rs               # RtcPeerConnection & RtcDataChannel engine
│       ├── invite.rs               # Offline HMAC invite generation & fragment packing
│       ├── bootstrap.rs            # Joiner auto-drop & edge disconnect logic
│       ├── crypto.rs               # Per-message payload encryption & zeroization
│       ├── mesh.rs                 # In-band gossip signaling & multi-peer mesh
│       └── ui.rs                   # Ephemeral DOM rendering & presence badges
├── zero_network/                   # Stateless Cloudflare Worker & Durable Object
│   ├── Cargo.toml
│   ├── wrangler.toml               # Cloudflare configuration & DO bindings
│   └── src/
│       ├── lib.rs
│       ├── durable_object.rs       # WebSocket hibernation & host attachment
│       └── handlers.rs             # POST /drop/:do_id/:uuid & HMAC validation
├── openspec/
│   ├── config.yaml
│   ├── specs/                      # Canonical architecture documentation
│   │   └── overview.md
│   ├── roadmaps/                   # Milestone roadmaps
│   │   ├── serverless-p2p-chat.md
│   │   └── archive/
│   └── changes/                    # Task specifications
│       └── archive/                # Archived change proposals (ticket-001..023)
├── tests/                          # Root integration test suites
│   ├── e2e_handshake.rs            # Full loopback handshake & chat roundtrip
│   └── mesh_simulation_test.rs     # 3+ peer in-band mesh simulation suite
├── docs/                           # Project guides & deployment runbooks
│   └── deployment.md               # Production deployment & secret configuration
├── scripts/
│   └── deploy_dry_run.sh           # Local pre-flight WASM bundle assembly check
├── Cargo.toml                      # Top-level Cargo workspace
└── factory.yaml                    # Software Factory automation commands
```

---

## 3. Subsystem Architecture

### 3.1 Cryptographic Primitives (`crates/zero_protocol::crypto`)
- **Key Exchange**: X25519 Diffie-Hellman (`x25519-dalek`) generating 32-byte ephemeral public/secret keypairs and deriving pairwise symmetric shared secrets ($K = \text{X25519}(sk_A, pk_B)$).
- **Symmetric AEAD**: Pure-Rust ChaCha20-Poly1305 (`chacha20poly1305`) providing authenticated encryption with 12-byte (96-bit) nonces generated via CSPRNG `getrandom` and 16-byte Poly1305 authentication tags. Plaintext is never placed on the wire in `ChatMessage.body`; all application payloads are sealed inside `SealedFrame { nonce, ciphertext }`.
- **Integrity & Token Sealing**: RFC 4231 constant-time HMAC-SHA256 (`hmac`, `sha2`) sealing offline room invites:
  $$\text{Token} = \text{HMAC-SHA256}(\text{host\_secret}, \text{Invite\_UUID})$$
- **Memory Zeroization**: Volatile memory zeroization on drop (`Zeroize`, `ZeroizeOnDrop`) ensures ephemeral keys and plaintext buffers are wiped immediately upon deallocation.

### 3.2 WebRTC Engine (`zero_client::webrtc`)
- Implements pure Rust WebAssembly abstractions over `web_sys::{RtcPeerConnection, RtcDataChannel}`.
- Non-trickle candidate gathering: Hooks `onicecandidate` and waits until `IceGatheringState::Complete`, assembling a fully self-contained SDP offer/answer bundle with deduplicated CRLF candidates.
- Safe lifecycle management: Rust closures bound to JavaScript callbacks (`onopen`, `onmessage`, `onerror`, `onclose`) are stored in an explicit registry and cleanly dropped on disconnection, preventing WASM memory leaks.

### 3.3 Hibernating Durable Object Dead-Drop (`zero_network`)
- **WebSocket Hibernation**: Room hosts connect to `/host/:do_id` via WebSocket. The Durable Object serializes the room state (`host_secret`) using `serialize_attachment()` and hibernates immediately, billing $0 during idle waiting.
- **Dead-Drop Endpoint (`POST /drop/:do_id/:uuid`)**: The joining peer posts its SDP answer directly to the Cloudflare Worker edge. The handler performs constant-time HMAC verification against the bearer token.
- **Single-Use Burning**: Upon successful delivery to the hibernating host WebSocket, the UUID is permanently burned in Durable Object storage. Any replay attempt receives an immediate `410 Gone`.

### 3.4 Stateless URL Fragment Auto-Drop (`zero_client::invite`, `bootstrap`)
- Alice generates an offline room invite bundle containing: `do_id`, `host_pubkey`, `invite_uuid`, `hmac_token`, and Alice's bundled SDP offer.
- The bundle is compressed/serialized into URL fragment `#payload=<base64>` and shared out-of-band. URL fragments are never transmitted to cloud servers in HTTP headers.
- When Bob opens the URL, the client automatically extracts the fragment, verifies the HMAC token offline, generates an SDP answer, and dispatches the dead-drop POST request.
- **Immediate Edge Severance**: As soon as the WebRTC DataChannel fires `onopen`, Alice and Bob explicitly close all signaling sockets.

### 3.5 Ephemeral Chat UI (`zero_client::ui`, `style.css`)
- Glassmorphic dark-mode interface with translucent panels (`backdrop-filter: blur(16px)`), pulsing connection badges, and responsive layouts.
- Domain URL branding in top-bar header and `<title>` (`zero-chat.pages.dev`) with embedded SVG favicon matching the static brand icon (`<span class="brand-icon">0</span>`).
- Header controls feature fixed bounding alignment with an adaptive split badge architecture:
  - **Disconnected State**: Standalone peer counter badge (`#peer-count`, displaying `0`) on the left and a double-width blue-dot idle capsule (`#idle-status-capsule`, 58px x 26px with centered static cyan/blue dot and non-pulsing animation) on the right.
  - **Connected State**: The idle capsule cleanly splits into the middle connection status pill (`#peer-list` / `#status-badge`) and the right circular 26px red exit button (`#header-leave-btn`), ensuring zero horizontal layout shifting across states.
  - **Header Add-Peer Button (`#header-add-btn`)**: Placed in `.header-controls` directly to the left of `#peer-count`. Styled with glassmorphic border and 14x14 plus SVG icon. Revealed during active room connections; hidden when disconnected or when peer count reaches 0. Clicking generates a single invite offer and silently copies the link to the clipboard without rendering any tooltips. If an invite is already pending, subsequent clicks re-copy the valid link without spawning duplicate WebRTC connections or cloud sessions. The pending invite clears upon peer connection.
- Clean in-feed transient status messaging: Connection progress displays `"Establishing secure connection"` with an animated CSS ticking ellipsis (`.`, `..`, `...`) without timestamps, cleanly replaced in-place when permanent system messages are appended (`Joined chat`, `Peer joined chat`, or `Failed to establish connection`).
- Direct chat creation logging: Room creation immediately outputs `"Secure chat created"` with timestamp.
- Minimalist welcome splash with centered brand headline (`Zero accounts. Zero servers. Zero history.`), 3-step action flow (`Create Chat -> Share Link -> Talk P2P`), and 3 invariant feature cards (`Direct P2P Connection` with description `"Private channels with no intermediate relays or storage servers."`, `Secure Communications`, `Ephemeral Sessions`) displayed strictly when the chat is empty; session burn and leave return the user cleanly to this home view.
- Unified chat bubble styling across incoming and outgoing messages with left accent borders (emerald for self, cyan/palette for peers), `[dot] [timestamp]` header layout with regular arrow cursor, and clean message bodies without bulky verification footers.
- Message bubbles are stamped with `data-peer-id="<peer_id>"` (or `"local"` for self) for deterministic author attribution and selective burning. Distinct peer colors are assigned from `PEER_COLOR_PALETTE`.
- Distinct Leave and Burn UI triggers:
  - Circular red exit button (`#header-leave-btn`): triggers selective peer message purge across surviving peers (`PeerLeftAndBurn`) and resets local view to home.
  - Session burn button (`#destroy-session-button`): triggers global room annihilation (`GlobalBurnSession`) flooding all peers and resetting everyone to home.
- Left-aligned system messages with monospace timestamps (`[HH:MM]`).

### 3.6 Decentralized In-Band Gossip Mesh & Lifecycle (`zero_client::mesh`, `envelope`)
- **Multi-Peer Connection Pool**: `ZeroWebRtcEngine` in `zero_client/index.html` manages concurrent WebRTC peers (`Map<peerId, { pc, dc }>`), supporting broadcast messaging (`broadcast_text`) across all active DataChannels and independent connection lifecycle tracking.
- **Opaque Bridge Relay (REQ-006.1, REQ-026.1)**: Room host acts as an in-band signaling bridge between existing peers (`Channel_AB`, `Channel_AC`). Relay frames (`RelaySdpOffer`, `RelaySdpAnswer`) contain end-to-end sealed payloads; relay nodes forward them verbatim without inspection, decrementing TTL on forwarding hops.
- **Direct Mesh Snapping (REQ-006.2, REQ-026.2)**: Secondary peers (Bob and Charlie) exchange SDP offers/answers over Alice's bridge and snap direct WebRTC DataChannels (`Channel_BC`) with **0 cloud edge requests**. Automatic host introductions (`PeerJoined`) and lexicographical glare tie-breakers ensure deterministic channel establishment.
- **Broadcast Flooding & Loop Suppression (REQ-006.3, REQ-006.4, REQ-026.3)**: Chat messages flood across direct links with initial TTL = 3. Network loops in cyclic/triangle topologies are suppressed using an LRU deduplication cache (`seenMessageIds`, 1000 entries), delivering messages exactly once.
- **Selective Leave & Purge (`PeerLeftAndBurn`)**: Clean departures broadcast `PeerLeftAndBurn` across the mesh before connection severance. Remote peers update routing tables and peer counts, selectively purge all DOM bubbles matching `.message-bubble[data-peer-id="<peer_id>"]`, and log a single clean `"Peer left chat"` notice (REQ-024.1). In multi-peer rooms, when 1 of N peers leaves, the room remains active for surviving peers with a decremented counter.
- **Global Room Annihilation (`GlobalBurnSession`)**: Triggered via `#destroy-session-button`, flooding a burn signal across all active DataChannels. All recipient nodes immediately invoke `resetToHome()`, closing connections, zeroizing memory, and wiping feeds simultaneously.
- **Dirty Disconnect, Window Unload Purge & Controls Preservation**: Abrupt channel closures (`PeerLifecycleState::Closed` / `st === "closed"`) and `MeshTopology::remove_peer` automatically invoke `on_burn_peer_messages` to purge disconnected peer messages from the UI feed. Window `beforeunload` and `pagehide` listeners opportunistically dispatch `leave_and_burn` signals out before browser exit with deduplication guards. On peer departure, surviving nodes preserve disconnected room controls: header peer count `0`, padlock icon, and leave button remain visible (blue dot hidden), burn button remains accessible below header, and message input is disabled with `"Peer left chat"` placeholder, allowing the user to cleanly leave or burn to return to the home view (REQ-024.3, REQ-024.4).

### 3.7 CI/CD Automation (`.github/workflows/`)
- **`ci.yml`**: Triggers on pull requests and pushes to `main`. Executes `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, full workspace test suite (`cargo test --workspace`), and target-specific builds (`wasm32-unknown-unknown` for `zero_client`).
- **`deploy.yml`**: Production CD triggering on pushes to `main`. Uses `cloudflare/wrangler-action@v3` to deploy `zero_network` Worker + Durable Object bindings and publish the assembled `zero_client` WASM bundle to Cloudflare Pages. Enforces `cancel-in-progress: false` to protect active deployments.

---

## 4. End-to-End Handshake & Messaging Sequence

```text
Host (Alice)                     Joiner (Bob)                 Cloudflare DO
    |                                 |                             |
    |-- 1. Create DO room & WS ------>|                             |
    |   (stores host_secret)          |                             |
    |                                 |                             |-- [DO Hibernates $0/sec]
    |-- 2. Offline Invite Bundle ---->|                             |
    |   (URL #payload= fragment)      |                             |
    |                                 |-- 3. Extract & verify HMAC -|
    |                                 |-- 4. POST /drop/:do_id/:uuid -> [Wakes DO]
    |                                 |      (contains SDP answer)  |-- Burn UUID (410)
    |<-- 5. Pipe Answer via WS -------------------------------------|
    |                                 |                             |
    |==== 6. WebRTC DataChannel Open (UDP P2P Direct) =============|
    |                                 |                             |
    |-- 7. Close WebSocket ---------->|                             |
    |   [EDGE SEVERED: 0 CLOUD HITS]  |                             |
    |                                 |                             |
    |<=== 8. Encrypted ChaCha20-Poly1305 Chat (P2P Direct) =======>|
```

---

## 5. Test & Validation Coverage (113 Tests)

```text
Unit Tests:
  - zero_client lib (58 tests):
    * bootstrap & auto-drop: 8 passed
    * payload sealing & zeroization: 13 passed
    * invite generation & fragment packing: 8 passed
    * mesh topology & in-band relay: 11 passed (including leave and burn broadcasts, disconnect cleanup)
    * chat UI & zero-persistence audit: 18 passed (including selective message burning, multi-peer partial departures, dynamic peer count, burn idempotency, multi-remote peer selective purge, dynamic peer counter lifecycle & symmetry)
  - zero_network lib (3 tests):
    * ephemeral room capacity & burn: 2 passed
    * mock host socket attachment: 1 passed
  - zero_protocol lib (8 tests):
    * envelope schemas & TTL decrementation: 8 passed (including PeerLeftAndBurn, GlobalBurnSession)

Integration Test Suites:
  - tests/e2e_handshake.rs (5 tests):
    * REQ-005.1: Ephemeral UI lifecycle & status transitions
    * REQ-005.2: Symmetric payload sealing & packet inspection
    * REQ-005.3: Tamper detection & security alert handling
    * REQ-005.4: Zero persistence lifecycle audit
    * REQ-005.5: Complete E2E handshake & encrypted chat roundtrip
  - tests/mesh_simulation_test.rs (14 tests):
    * REQ-006.1: Opaque bridge signaling relay
    * REQ-006.2: Direct mesh snap zero cloud edge
    * REQ-006.3: Mesh broadcast flooding delivery
    * REQ-006.4: Duplicate broadcast loop suppression & bounded cache
    * REQ-006.5: Peer disconnect & UI cleanup
    * REQ-007.1: Leave and burn selective message purging
    * REQ-007.2: Global burn session room annihilation
    * REQ-007.3: Dirty disconnect automatic peer message purging
    * REQ-028.1: Multi-node connected mesh simulation (triangular snap, 0 edge hits, broadcast flooding, loop suppression)
    * REQ-028.2: Partial peer departure & feed survivability (selective purge, surviving channel retention)
    * REQ-029.4: Third peer departure selective message purge
    * REQ-029.5: Host departure pairwise channel survivability and feed continuity
    * REQ-030.1 / REQ-030.2: Secondary inviter mesh snapping and counter synchronization across all nodes
    * REQ-030.3 / REQ-030.4: Symmetric departure counter decrement across surviving peers and host departure survivability
  - tests/invite_tests.rs (4 tests):
    * REQ-004.1 through REQ-004.5
  - tests/webrtc_tests.rs (6 tests):
    * REQ-002.1 through REQ-002.5
  - tests/network_tests.rs (5 tests):
    * REQ-003.1 through REQ-003.5
  - tests/protocol_tests.rs (10 tests):
    * REQ-001.1 through REQ-001.5

Total: 113 passed, 0 failed, 0 ignored.
```

