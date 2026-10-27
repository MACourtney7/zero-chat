# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-049"
title: "Ephemeral Cloudflare Realtime TURN Relay for 4G/5G CGNAT Traversal"
state: "PLANNING"
created_at: "2026-10-27T21:00:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-049/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

While previous milestones resolved smart STUN (`typ srflx`) candidate gathering, per-peer cryptographic sealing (`WasmCryptoSession`), and transient background disconnect grace windows, direct peer-to-peer UDP hole-punching via STUN is physically blocked in two common mobile networking scenarios:
1. **Carrier-Grade Symmetric NAT (CGNAT) on 4G/5G Cellular Networks**: Mobile carriers (EE, Vodafone, O2, Three, etc.) assign a distinct external UDP port per destination IP/port tuple. Because the port mapped during the STUN binding request to `stun.cloudflare.com:3478` cannot accept incoming UDP packets from a remote peer's IP, `typ srflx` candidates fail to connect when both devices are on cellular data or behind strict corporate/symmetric NATs.
2. **Same-Device Multi-Tab Cellular Hairpinning**: When operating or testing two browser tabs on the same mobile device over cellular data, mobile OS sandboxes suppress local loopback (`host`) candidates for privacy, and carrier NATs drop hairpinned UDP packets sent from a public cellular IP back to itself.

`ticket-049` implements Zero-Knowledge Ephemeral TURN Relay Provisioning using Cloudflare Calls / Realtime TURN via `zero_network`, wires dynamic ICE server injection and 15-minute caching into `zero_client/index.html`, and tunes non-trickle candidate gathering to ensure `typ relay` candidates are bundled when TURN is configured.

### Privacy & Cryptographic Invariants
Using a TURN relay (`typ relay`) does not weaken ZeroChat's cryptographic privacy:
- WebRTC encrypts the underlying SCTP DataChannel transport via DTLS.
- `WasmCryptoSession` encrypts every payload end-to-end using pairwise X25519 + ChaCha20-Poly1305 AEAD signed with Ed25519.
- The Cloudflare TURN server sees only opaque, doubly-encrypted UDP/TCP frames and short-lived ephemeral credentials with zero room, user, or message metadata.

---

## 2. Requirements & Acceptance Criteria

### REQ-049.1: Ephemeral TURN Credential Endpoint in `zero_network` (`GET /ice`)
- In `zero_network/src/handlers.rs` and `zero_network/src/lib.rs`:
  - Expose `GET /ice` and `OPTIONS /ice` routes with full CORS support (`Access-Control-Allow-Origin: *`).
  - Support three configuration modes:
    - **Mode 1 (Cloudflare Realtime TURN API)**: When `CF_TURN_KEY_ID` and `CF_TURN_API_TOKEN` are present in Worker `env`, issue an outbound HTTPS POST from the Worker to `https://rtc.live.cloudflare.com/v1/turn/keys/${CF_TURN_KEY_ID}/credentials/generate` with header `Authorization: Bearer ${CF_TURN_API_TOKEN}` and body `{"ttl": 3600}`. Return the generated `{ "iceServers": [...] }` directly with `200 OK`.
    - **Mode 2 (Static/Custom JSON Override)**: If `TURN_SERVERS_JSON` is present in `env`, parse and return `{ "iceServers": <parsed> }`.
    - **Mode 3 (Graceful STUN-Only Fallback)**: If neither secret is configured (or if upstream Cloudflare TURN API fails), return `200 OK` with the default STUN servers:
      ```json
      {
        "iceServers": [
          { "urls": "stun:stun.cloudflare.com:3478" },
          { "urls": "stun:stun.l.google.com:19302" }
        ]
      }
      ```
- In `zero_network/wrangler.toml`:
  - Document optional environment variable bindings (`CF_TURN_KEY_ID`, `CF_TURN_API_TOKEN`, `TURN_SERVERS_JSON`).

### REQ-049.2: Dynamic ICE Server Provisioning & Caching in `zero_client/index.html`
- Implement `ensureIceServersReady()` in `zero_client/index.html`:
  - Fetches `${WORKER_HTTP_ORIGIN}/ice` with an `AbortController` timeout of 2,500ms.
  - Caches valid `iceServers` in memory for 15 minutes (`ICE_CACHE_TTL_MS = 15 * 60 * 1000`) so rapid multi-peer handshakes avoid repeated HTTP requests.
  - Merges default STUN servers + fetched `iceServers` + `window.ZERO_TURN_SERVERS`.
  - In `ZeroWebRtcEngine`: add `setIceServers(iceServers)` to update `this.rtcConfig.iceServers`.
  - Invoke `await ensureIceServersReady()` alongside `ensureCryptoReady()` in `createRoomAsHost()`, `handleHeaderAddPeerClick()`, and `joinRoomFromHash()`.

### REQ-049.3: Smart Relay Settle Logic for Non-Trickle ICE Candidate Bundling
- In `_waitForIceGatheringComplete(pc)` in `zero_client/index.html`:
  - Detect whether any TURN server is configured:
    ```javascript
    const hasTurnConfigured = (this.rtcConfig.iceServers || []).some((s) => {
        const urls = Array.isArray(s.urls) ? s.urls : [s.urls || ""];
        return urls.some((u) => typeof u === "string" && (u.startsWith("turn:") || u.startsWith("turns:")));
    });
    ```
  - **When `hasTurnConfigured === false`**: Trigger the 400ms settle timer as soon as the first `typ srflx` (or `typ relay`) candidate is gathered.
  - **When `hasTurnConfigured === true`**:
    - Trigger a fast 350ms settle timer once at least one `typ relay` candidate is gathered.
    - If only `typ srflx` candidates arrive (e.g. TURN server is slow or unreachable), trigger fallback settle after 1,200ms.
    - Retain the existing 5,000ms hard ceiling.
  - Guarantees that non-trickle SDP bundles both direct `typ srflx` and relay `typ relay` candidates.

### REQ-049.4: Automated Verification & Test 23 Suite
- **Rust Unit & Integration Tests**:
  - Add tests in `zero_network` validating `GET /ice` response schemas in default fallback mode and mock configured mode.
  - Ensure all 115+ workspace tests pass with 0 failures under `cargo test --workspace`.
- **Browser Headless Suite (Test 23)**:
  - Add Test 23 in `zero_client/tests/browser/test_mesh_scenario.js` (and mock support in `mock_environment.js`):
    - **Scenario A**: `/ice` endpoint fallback & dynamic ICE provisioning (`ensureIceServersReady`).
    - **Scenario B**: 15-minute cache window preventing redundant HTTP fetches.
    - **Scenario C**: `hasTurnConfigured` relay settle logic ensuring `typ relay` candidates are bundled in non-trickle SDP.
    - **Scenario D**: Full 3-peer encrypted mesh formation with simulated TURN relay candidates.

### REQ-049.5: Production Deployment Guide & Spec Reconciliation
- Update `openspec/specs/overview.md` with **Invariant 25** (Zero-Knowledge Ephemeral TURN Relay & CGNAT Traversal).
- Document operator activation instructions:
  ```bash
  wrangler secret put CF_TURN_KEY_ID
  wrangler secret put CF_TURN_API_TOKEN
  ```

---

## 3. Cognitive Sizing & Brief Decomposition

| Brief ID | Subsystem Target | Files Impacted | Cognitive Level |
| :--- | :--- | :--- | :---: |
| `brief-impl-049-network` | `/ice` Worker Endpoint & Wrangler Configuration | `zero_network/src/handlers.rs`, `zero_network/src/lib.rs`, `zero_network/wrangler.toml`, `zero_network/tests/network_tests.rs` | `MEDIUM` [M] |
| `brief-impl-049-client` | Dynamic ICE Provisioning & Smart Relay Settle | `zero_client/index.html` | `MEDIUM` [M] |
| `brief-impl-049-tests` | Headless Test 23 Suite & Mock Environment | `zero_client/tests/browser/mock_environment.js`, `zero_client/tests/browser/test_mesh_scenario.js` | `MEDIUM` [M] |

---

## 4. Verification Commands

```bash
cargo fmt -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/deploy_dry_run.sh
bash zero_client/tests/browser/run_test.sh
```
