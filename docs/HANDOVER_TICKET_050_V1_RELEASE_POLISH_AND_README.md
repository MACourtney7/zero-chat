Architectural Handover Brief: v1.0 Release Polish — WASM Binary Optimization (wasm-opt -Oz) & Root README.md Showcase (ticket-050)
1. Executive Summary & Objective
With ticket-045 through ticket-049 shipped, ZeroChat's core architecture, zero-knowledge cryptography, multi-peer mesh state machine, and mobile CGNAT traversal are complete and verified in production (v1.23.1).

ticket-050 closes out the v1.0 milestone with two final release-engineering deliverables:

WASM Size & Release Profile Optimization (Cargo.toml & scripts/deploy_dry_run.sh): Configure [profile.release] for aggressive LTO/size stripping and integrate an automatic wasm-opt -Oz pass (when wasm-opt is available on PATH, with graceful fallback when absent) into the build pipeline to shrink zero_client_bg.wasm to its minimal wire footprint.
Production Architectural Showcase (README.md): Author a comprehensive, staff-level root README.md that documents the v1.0 architecture, the Option B Modernized Burner Keybundle (X25519 + Ed25519 + ChaCha20-Poly1305 + ZeroizeOnDrop), the Blind Dead-Drop handshake, the Self-Healing Multi-Peer Mesh, and the 3-tier Cloudflare Realtime TURN CGNAT fallback.


2. Target File Scope
Cargo.toml (Add/refine workspace [profile.release] settings: opt-level = "z", lto = true, codegen-units = 1, panic = "abort", strip = true)
scripts/deploy_dry_run.sh (Add post-wasm-bindgen wasm-opt -Oz pass on zero_client/dist/pkg/zero_client_bg.wasm if command -v wasm-opt succeeds, printing before/after byte sizes)
README.md (Create or update the root README.md to showcase the v1.0 production system)
openspec/specs/overview.md (Bump release status to v1.0.0 / ticket-050 complete)


3. Detailed Technical Specification
Part A: Release Profile & wasm-opt -Oz Build Pipeline
Workspace Cargo.toml [profile.release] Configuration: Ensure the workspace root Cargo.toml includes a dedicated [profile.release] block optimized for minimal WASM binary size and dead-symbol elimination:

[profile.release]

opt-level = "z"

lto = true

codegen-units = 1

panic = "abort"

strip = true

(Note: Verify cargo test --workspace and cargo build --target wasm32-unknown-unknown --release both compile cleanly with these flags.)

Post-Bindgen wasm-opt Pass in scripts/deploy_dry_run.sh: Immediately after wasm-bindgen generates zero_client/dist/pkg/zero_client_bg.wasm, add an optimization step that checks for wasm-opt (from Binaryen):

WASM_FILE="zero_client/dist/pkg/zero_client_bg.wasm"

if [ -f "$WASM_FILE" ]; then

  BEFORE_BYTES=$(wc -c < "$WASM_FILE" | tr -d ' ')

  echo "[deploy_dry_run] WASM size before wasm-opt: ${BEFORE_BYTES} bytes"

  if command -v wasm-opt >/dev/null 2>&1; then

    wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int "$WASM_FILE" -o "$WASM_FILE"

    AFTER_BYTES=$(wc -c < "$WASM_FILE" | tr -d ' ')

    echo "[deploy_dry_run] WASM size after wasm-opt -Oz: ${AFTER_BYTES} bytes"

  else

    echo "[deploy_dry_run] wasm-opt not found on PATH; skipping Binaryen post-pass (Cargo LTO/strip active)"

  fi

fi


Part B: Root README.md Architectural Showcase
Create/replace the root README.md with a polished, technical showcase covering the following sections:

Header & Executive Summary:
ZeroChat (v1.0) — Ephemeral, Zero-Knowledge Peer-to-Peer Mesh Chat in Rust & WebAssembly.
No accounts, no databases, no message persistence, and a 100% blind signaling edge.
System Architecture (3-Crate Workspace Overview):
ASCII / Mermaid architecture diagram illustrating the boundary between:
crates/zero_protocol (Pure Rust): Wire envelopes (InvitePayload, DropPayload, SealedFrame, MeshRelaySignal) and cryptographic primitives.
zero_network (Cloudflare Worker + Hibernating Durable Objects in Rust/WASM): Stateless HMAC-authenticated single-use dead-drop router (/register, /drop, /ws) and 3-tier ephemeral TURN credential provisioning (/ice).
zero_client (Rust WASM WasmCryptoSession + Vanilla JS WebRTC/DOM Host): High-speed WASM linear-memory cryptography paired with browser-native WebRTC DataChannels and mobile visualViewport resilience.
Cryptographic Design (Option B Modernized Burner Keybundle):
In-Memory Ephemeral Identity: Every session generates a fresh BurnerKeybundle (x25519-dalek Diffie-Hellman keypair + ed25519-dalek signing keypair) in WASM linear memory protected by ZeroizeOnDrop.
Out-of-Band #payload= Invite Fragment: The Host's public keys and HMAC token travel exclusively inside the URL fragment (#payload=...), which is never transmitted in HTTP requests to Cloudflare Pages or the Worker.
100% Blind Dead-Drop (sealed_sdp_answer): Before the Joiner sends their WebRTC SDP answer to POST /drop/:do_id/:invite_uuid, WasmCryptoSession::seal_drop_sdp encrypts the SDP answer and ICE IP candidates using X25519 + ChaCha20-Poly1305 AEAD and signs the payload with Ed25519. The Cloudflare Worker sees only sdp_answer: { sdp: "SEALED" } and an opaque ciphertext blob.
Per-Peer AEAD & Signed Multi-Peer Mesh: Every BroadcastChat, RelaySdpOffer, and RelaySdpAnswer frame is uniquely sealed per peer with a 96-bit CSPRNG nonce, 128-bit Poly1305 MAC tag, and 64-byte Ed25519 signature.
Deterministic Burn & Memory Zeroization: Clicking Leave (PeerLeftAndBurn) or closing the tab immediately purges remote DOM messages, drops peer keys, and invokes burn_all() to scrub secret material from WASM RAM.
Network & Mobile Resilience Engineering:
Smart Non-Trickle ICE Gathering: Adaptive candidate inspection resolves in <600ms on broadband/Wi-Fi once typ srflx is gathered, or waits for typ relay (350ms settle window) when TURN is configured.
Mobile Background Tab Freeze Grace Window (12s): Distinguishes explicit PeerLeftAndBurn departures (instant purge) from iOS Safari / Android Chrome background tab suspension (visibilitychange recovery without false departure notices or message loss).
4G/5G Symmetric CGNAT Traversal (/ice): Three-tier ICE provisioning supporting dynamic 1-hour Cloudflare Realtime TURN credentials (CF_TURN_KEY_ID + CF_TURN_API_TOKEN), custom JSON overrides (TURN_SERVERS_JSON), or automatic zero-config STUN fallback.
Local Development, Testing & Deployment Guide:
Exact commands for running the 117+ Rust workspace unit/integration tests (cargo test --workspace), the 23 headless browser E2E scenarios (node scripts/test_mesh_scenario.js), building the optimized WASM bundle (./scripts/deploy_dry_run.sh), and deploying to Cloudflare Workers & Pages.


4. Verification & Acceptance Criteria
cargo test --workspace — All Rust unit and integration tests pass with 0 failures.
cargo clippy --workspace --all-targets -- -D warnings — Zero warnings.
./scripts/deploy_dry_run.sh — Builds the release WASM binary with [profile.release] (opt-level = "z", lto = true, strip = true) and optional wasm-opt -Oz, outputting the final .wasm byte size.
node scripts/test_mesh_scenario.js — All 23 headless browser scenarios pass against the size-optimized release WASM build.
