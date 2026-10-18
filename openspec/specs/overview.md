# Canonical System Architecture: zero_chat

```yaml
---
system: "zero_chat"
version: "1.0.0"
milestone: "serverless-p2p-chat"
status: "PRODUCTION_CANONICAL"
updated_at: "2026-10-18T16:34:00Z"
---
```

## 1. Project Overview & Vision
`zero_chat` is an entirely serverless, zero-footprint, real-time peer-to-peer (P2P) communication platform operating inside web browsers using pure Rust compiled to WebAssembly (`wasm32-unknown-unknown`), coupled with an ephemeral Cloudflare Worker / Hibernating Durable Object signaling bootstrap utilizing WebSocket Hibernation, and automated dual-target CI/CD delivery.

### Core Architectural Invariants
1. **Zero Forensic Footprint**: No account registrations, no cookies, no `localStorage`, `sessionStorage`, or `indexedDB` persistence. Closing or reloading the browser tab immediately incinerates all in-memory keys, sessions, and chat feeds.
2. **Stateless HMAC-Sealed Handshake (Pattern B)**: Signaling dead-drop HTTP `POST` requests and hibernating WebSockets ($0 idle duration billing). Offline invites are sealed with `HMAC_SHA256(host_secret, Invite_UUID)` embedded in URL `#fragment`s, which never touch any cloud edge.
3. **Pure Rust Cryptographic Engine**: Synchronous, memory-safe cryptographic primitives (`x25519-dalek`, `chacha20poly1305`, `hmac-sha256` with `getrandom/js`) shared across WASM and Cloudflare Worker, bypassing asynchronous JS WebCrypto bridging overhead.
4. **Direct P2P DataChannels**: Once the initial WebRTC SDP offer/answer exchange completes, the cloud edge drops out permanently (`is_edge_severed() == true`). All communication flows directly over browser-to-browser UDP DataChannels.
5. **In-Band Gossip Mesh**: Multi-peer scaling without cloud edge requests. The room host acts as an opaque in-band signaling bridge between existing DataChannels (`Channel_AB`, `Channel_AC`), allowing joining peers to snap direct connections (`Channel_BC`) with zero edge hits.
6. **Hermetic CI/CD Pipelines**: Automated dual-target test validation, strict clippy auditing, and automated Cloudflare Worker + Cloudflare Pages deployment via GitHub Actions.

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
│       └── archive/                # Archived change proposals (ticket-001..018)
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
  - **Disconnected State**: Standalone peer counter badge (`#peer-count`, displaying `0`) on the left and a double-width red-dot idle capsule (`#idle-status-capsule`, 58px x 26px with centered red glowing dot) on the right.
  - **Connected State**: The idle capsule cleanly splits into the middle connection status pill (`#peer-list` / `#status-badge`) and the right circular 26px red exit button (`#header-leave-btn`), ensuring zero horizontal layout shifting across states.
- Clean in-feed transient status messaging: Connection progress displays `"Establishing secure connection"` with an animated CSS ticking ellipsis (`.`, `..`, `...`) without timestamps, cleanly replaced in-place when permanent system messages are appended (`Joined chat`, `Peer joined chat`, or `Failed to establish connection`).
- Direct chat creation logging: Room creation immediately outputs `"Secure chat created"` with timestamp.
- Minimalist welcome splash with centered brand headline (`Zero accounts. Zero servers. Zero history.`), 3-step action flow (`Create Chat -> Share Link -> Talk P2P`), and 3 invariant feature cards (`Direct P2P Connection` with description `"Private data channels with no intermediate relays or storage servers."`, `Secure Communications`, `Ephemeral Sessions`) displayed strictly when the chat is empty; session burn and leave return the user cleanly to this home view.
- Unified chat bubble styling across incoming and outgoing messages with left accent borders (emerald for self, cyan/palette for peers), `[dot] [timestamp]` header layout with regular arrow cursor, and clean message bodies without bulky verification footers.
- Left-aligned system messages with monospace timestamps (`[HH:MM]`).

### 3.6 Decentralized In-Band Gossip Mesh (`zero_client::mesh`, `envelope`)
- **Opaque Bridge Relay (REQ-006.1)**: Room host acts as an in-band signaling bridge between existing peers (`Channel_AB`, `Channel_AC`). Relay frames (`RelaySdpOffer`, `RelaySdpAnswer`) contain end-to-end sealed payloads; relay nodes forward them verbatim without inspection.
- **Direct Mesh Snapping (REQ-006.2)**: Secondary peers (Bob and Charlie) exchange SDP offers/answers over Alice's bridge and snap direct WebRTC DataChannels (`Channel_BC`) with **0 cloud edge requests**.
- **Broadcast Flooding & Loop Suppression (REQ-006.3, REQ-006.4)**: Chat messages flood across direct links with initial TTL = 3. Network loops in cyclic/triangle topologies are suppressed using a FIFO message ID cache (`seen_messages`), delivering messages exactly once.
- **Graceful Teardown (REQ-006.5)**: Peer disconnects are broadcast via `PeerLeft` signals, cleaning routing tables and updating UI peer indicator lists while keeping remaining links active.

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

## 5. Test & Validation Coverage (91 Tests)

```text
Unit Tests:
  - zero_client lib (47 tests):
    * bootstrap & auto-drop: 8 passed
    * payload sealing & zeroization: 12 passed
    * invite generation & fragment packing: 8 passed
    * mesh topology & in-band relay: 8 passed
    * chat UI & zero-persistence audit: 11 passed
  - zero_network lib (3 tests):
    * ephemeral room capacity & burn: 2 passed
    * mock host socket attachment: 1 passed
  - zero_protocol lib (6 tests):
    * envelope schemas & TTL decrementation: 6 passed

Integration Test Suites:
  - tests/e2e_handshake.rs (5 tests):
    * REQ-005.1: Ephemeral UI lifecycle & status transitions
    * REQ-005.2: Symmetric payload sealing & packet inspection
    * REQ-005.3: Tamper detection & security alert handling
    * REQ-005.4: Zero persistence lifecycle audit
    * REQ-005.5: Complete E2E handshake & encrypted chat roundtrip
  - tests/mesh_simulation_test.rs (5 tests):
    * REQ-006.1: Opaque bridge signaling relay
    * REQ-006.2: Direct mesh snap zero cloud edge
    * REQ-006.3: Mesh broadcast flooding delivery
    * REQ-006.4: Duplicate broadcast loop suppression & bounded cache
    * REQ-006.5: Peer disconnect & UI cleanup
  - tests/invite_tests.rs (4 tests):
    * REQ-004.1 through REQ-004.5
  - tests/webrtc_tests.rs (6 tests):
    * REQ-002.1 through REQ-002.5
  - tests/network_tests.rs (5 tests):
    * REQ-003.1 through REQ-003.5
  - tests/protocol_tests.rs (10 tests):
    * REQ-001.1 through REQ-001.5

Total: 91 passed, 0 failed, 0 ignored.
```
