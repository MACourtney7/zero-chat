# System Architecture: zero_chat

## 1. Project Overview & Vision
`zero_chat` is a serverless, zero-footprint, real-time peer-to-peer (P2P) communication application operating entirely inside web browsers using pure Rust compiled to WebAssembly (`wasm32-unknown-unknown`), coupled with an ephemeral Cloudflare Worker / Durable Object signaling bootstrap utilizing WebSocket Hibernation.

### Core Invariants
- **Zero Forensic Footprint**: No accounts, no cookies, no local storage or IndexedDB persistence. Closing the browser tab destroys all ephemeral keys, sessions, and message logs.
- **Stateless HMAC-Sealed Handshake (Pattern B)**: Signaling dead-drop HTTP `POST` requests and hibernating WebSockets ($0 idle duration billing). Offline invites are sealed with `HMAC_SHA256(host_secret, Invite_UUID)` embedded in URL `#fragment`s, which never hit any cloud edge.
- **Pure Rust Cryptographic Engine**: Synchronous, memory-safe cryptographic primitives (`x25519-dalek`, `chacha20poly1305`, `hmac-sha256` with `getrandom/js`) shared between WASM and Cloudflare Worker, bypassing asynchronous JS WebCrypto bridging overhead.
- **Direct P2P DataChannels**: Once the initial WebRTC SDP offer/answer exchange completes, the cloud edge drops out permanently. All communication flows directly over browser-to-browser UDP DataChannels.
- **In-Band Gossip Mesh**: Multi-peer scaling without edge requests. The room host bridges encrypted signaling messages between existing WebRTC DataChannels so joining peers snap direct connections (`Channel_BC`) with zero edge hits.

---

## 2. Directory Layout
```text
zero_chat/
├── .agents/                        # Antigravity IDE configuration & plugins
│   ├── plugins.json                # Software factory plugin reference
│   └── mcp_config.json             # MCP server configuration
├── .factory/                       # Factory state machine runtime (gitignored)
├── crates/
│   └── zero_protocol/              # Shared schemas, DTOs, & crypto primitives
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── crypto.rs           # HMAC-SHA256, X25519, ChaCha20-Poly1305
│           └── envelope.rs         # Signaling & chat envelopes
├── zero_client/                    # Pure Rust WASM client & WebRTC engine
│   ├── Cargo.toml
│   ├── index.html                  # Zero-npm HTML/CSS web frontend
│   ├── style.css                   # Minimalist / glassmorphic dark styling
│   └── src/
│       ├── lib.rs
│       ├── webrtc.rs               # RtcPeerConnection & RtcDataChannel engine
│       ├── invite.rs               # Offline HMAC invite generation & fragment packing
│       ├── bootstrap.rs            # Joiner auto-drop & edge disconnect logic
│       ├── crypto.rs               # Payload encryption & verification
│       ├── mesh.rs                 # In-band gossip signaling & multi-peer mesh
│       └── ui.rs                   # Ephemeral DOM rendering & event handling
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
│   │   └── serverless-p2p-chat.md
│   └── changes/                    # Active task specifications
│       ├── ticket-001/spec.md
│       ├── ticket-002/spec.md
│       ├── ticket-003/spec.md
│       ├── ticket-004/spec.md
│       ├── ticket-005/spec.md
│       └── ticket-006/spec.md
├── tests/                          # End-to-end integration test suites
│   ├── e2e_handshake.rs
│   └── mesh_simulation_test.rs
├── Cargo.toml                      # Top-level Cargo workspace
└── factory.yaml                    # Software Factory automation commands
```

---

## 3. SDD Workflow & Automation
All feature development proceeds through the OpenSpec SDD workflow:
- Specifications reside in `openspec/specs/` and active proposals in `openspec/changes/`.
- Milestones and DAG roadmaps reside in `openspec/roadmaps/`.
- Work is coordinated via the Agentic Software Factory with isolated worktrees.
- Autonomous execution is triggered via `implement-roadmap` or `implement-task <ticket-id>`.
