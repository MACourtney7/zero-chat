Architectural Handover Brief: Ephemeral Cloudflare Realtime TURN Relay for 4G/5G CGNAT Traversal (ticket-049)
1. Executive Summary & Objective
While ticket-045 through ticket-048 resolved smart STUN (srflx) candidate gathering, per-peer cryptographic sealing (WasmCryptoSession), and transient background disconnect grace windows, direct peer-to-peer UDP hole punching via STUN is physically blocked in two common mobile networking scenarios:

Carrier-Grade Symmetric NAT (CGNAT) on 4G/5G Cellular Networks: Mobile carriers (e.g., EE, Vodafone, O2, Three) assign a distinct external UDP port per destination IP/port tuple. Because the port mapped during the STUN request to stun.cloudflare.com:3478 cannot accept incoming UDP packets from a remote peer's IP, srflx candidates fail to connect when both devices are on cellular data (or behind strict enterprise firewalls).
Same-Device Multi-Tab Cellular Hairpinning: When testing or operating two browser tabs on the same mobile device over cellular data, mobile OS sandboxes suppress local loopback (host) candidates for privacy and carrier NATs drop hairpinned UDP packets sent from a public cellular IP back to itself.

ticket-049 implements Zero-Knowledge Ephemeral TURN Relay Provisioning using Cloudflare Calls / Realtime TURN via zero_network and wires dynamic ICE server injection into zero_client/index.html.
Privacy & Security Invariant
Using a TURN relay (typ relay) does not weaken ZeroChat's cryptographic privacy:

WebRTC encrypts the underlying SCTP DataChannel transport via DTLS, and WasmCryptoSession (ticket-046/047) encrypts every payload end-to-end using pairwise X25519 + ChaCha20-Poly1305 AEAD signed with Ed25519.
The TURN server sees only opaque, doubly-encrypted UDP/TCP frames and short-lived ephemeral credentials with zero room, user, or message metadata.


2. Target File Scope
zero_network/src/handlers.rs & zero_network/src/lib.rs (Add GET /ice or POST /ice endpoint to mint short-lived ephemeral Cloudflare TURN credentials—or return fallback STUN configuration smoothly when TURN secrets are not configured in the Worker environment)
zero_network/wrangler.toml (Document optional environment bindings CF_TURN_KEY_ID and CF_TURN_API_TOKEN / TURN_SERVERS_JSON for production deployment)
zero_client/index.html (Fetch ephemeral ICE/TURN servers from ${WORKER_HTTP_ORIGIN}/ice prior to creating RTCPeerConnection instances, merge with window.ZERO_TURN_SERVERS and default STUN servers, and tune _waitForIceGatheringComplete to wait for typ relay candidates when TURN is active)
scripts/test_mesh_scenario.js (Add automated verification for /ice endpoint fallback, dynamic iceServers injection into ZeroWebRtcEngine, and typ relay candidate bundling)


3. Detailed Technical Specification
Part A: Ephemeral TURN Credential Endpoint in zero_network (GET /ice)
Pure-Rust Route Handler (handle_ice_config in zero_network/src/handlers.rs):
Expose a lightweight endpoint GET /ice (with CORS headers matching /register and /drop) so browser clients can fetch short-lived iceServers configuration before initiating an offer or answer.
Support two zero-maintenance configuration modes via Cloudflare Worker environment variables (env):
Mode 1: Cloudflare Realtime TURN API (CF_TURN_KEY_ID + CF_TURN_API_TOKEN): When both secrets are present in the Worker environment, make an outbound POST request from the Worker to: https://rtc.live.cloudflare.com/v1/turn/keys/${CF_TURN_KEY_ID}/credentials/generate with header Authorization: Bearer ${CF_TURN_API_TOKEN} and body {"ttl": 3600} (1-hour ephemeral credentials). Return the resulting { "iceServers": [...] } JSON directly to the client with 200 OK.
Mode 2: Static/Custom JSON Override (TURN_SERVERS_JSON): If TURN_SERVERS_JSON is set in env, parse and return { "iceServers": <parsed> } (allowing easy injection of self-hosted coturn or Metered/OpenRelay credentials in staging).
Mode 3: Graceful STUN-Only Fallback (Default when unconfigured / local tests): If neither secret is configured (or if the upstream Cloudflare TURN API call fails), never fail the handshake! Return 200 OK with the default STUN server list:

{

  "iceServers": [

    { "urls": "stun:stun.cloudflare.com:3478" },

    { "urls": "stun:stun.l.google.com:19302" }

  ]

}

CORS Support:
Ensure OPTIONS /ice and GET /ice include Access-Control-Allow-Origin: * (matching existing endpoints in zero_network/src/lib.rs).


Part B: Dynamic ICE Server Provisioning & Caching (zero_client/index.html)
Fetch & Cache Ephemeral ICE Config (ensureIceServersReady):

In zero_client/index.html, add an async helper that fetches ${WORKER_HTTP_ORIGIN}/ice with a short 2500ms timeout and caches the returned iceServers in memory for 15 minutes (ICE_CACHE_TTL_MS = 15 * 60 * 1000) so rapid multi-peer mesh handshakes do not repeat HTTP requests:

let cachedIceServers = null;

let cachedIceTimestamp = 0;

const ICE_CACHE_TTL_MS = 15 * 60 * 1000;

async function ensureIceServersReady() {

  const now = Date.now();

  if (cachedIceServers && (now - cachedIceTimestamp) < ICE_CACHE_TTL_MS) {

    return cachedIceServers;

  }

  const defaultStun = [

    { urls: "stun:stun.cloudflare.com:3478" },

    { urls: "stun:stun.l.google.com:19302" },

    { urls: "stun:stun1.l.google.com:19302" }

  ];

  const customTurn = Array.isArray(window.ZERO_TURN_SERVERS) ? window.ZERO_TURN_SERVERS : [];

  try {

    const ctrl = new AbortController();

    const timer = setTimeout(() => ctrl.abort(), 2500);

    const res = await fetch(`${WORKER_HTTP_ORIGIN}/ice`, { signal: ctrl.signal });

    clearTimeout(timer);

    if (res.ok) {

      const data = await res.json();

      if (data && Array.isArray(data.iceServers) && data.iceServers.length > 0) {

        cachedIceServers = [...defaultStun, ...data.iceServers, ...customTurn];

        cachedIceTimestamp = now;

        engine.setIceServers(cachedIceServers);

        return cachedIceServers;

      }

    }

  } catch (_) {

    // Graceful fallback to STUN + window.ZERO_TURN_SERVERS when offline or in local mock tests

  }

  cachedIceServers = [...defaultStun, ...customTurn];

  cachedIceTimestamp = now;

  engine.setIceServers(cachedIceServers);

  return cachedIceServers;

}

Call await ensureIceServersReady() alongside await ensureCryptoReady() inside createRoomAsHost(), handleHeaderAddPeerClick(), and joinRoomFromHash().

Optimize _waitForIceGatheringComplete(pc) When TURN Servers Are Configured:

Inspect this.rtcConfig.iceServers to check whether any server URL starts with turn: or turns::

const hasTurnConfigured = (this.rtcConfig.iceServers || []).some((s) => {

  const urls = Array.isArray(s.urls) ? s.urls : [s.urls || ""];

  return urls.some((u) => typeof u === "string" && (u.startsWith("turn:") || u.startsWith("turns:")));

});

Why this matters for 4G/5G CGNAT:
When hasTurnConfigured === false (STUN only): as soon as the first typ srflx candidate arrives, the 400ms settle timer fires and resolves quickly (<600ms).
When hasTurnConfigured === true (TURN enabled): a typ srflx candidate often arrives in 80ms, whereas the TURN allocation (typ relay) takes ~250ms–700ms (requires a 401 challenge + authenticated Allocate response). If we resolved 400ms after srflx before typ relay arrived, the non-trickle SDP would omit the relay candidate!
Required Logic:
When hasTurnConfigured === true: trigger the fast 350ms settle timer once at least one typ relay candidate is gathered (or after 1200ms if typ srflx is present and TURN is unreachable, with the existing 5000ms hard cap).
This guarantees that whenever TURN is configured, both typ srflx (direct P2P UDP when possible) and typ relay (CGNAT/cellular fallback over UDP/TCP/TLS 443) are bundled into the non-trickle SDP!


4. Verification & Acceptance Criteria
Gate 1: Rust Workspace & Worker Endpoint Tests
cargo test --workspace — Add unit tests in zero_network verifying GET /ice returns valid iceServers JSON in both default fallback mode and configured mode, and passes all workspace tests.
cargo clippy --workspace --all-targets -- -D warnings — Zero warnings.
Gate 2: Headless Browser Verification (scripts/test_mesh_scenario.js)
Scenario 1 (/ice Provisioning & Caching): Verify ensureIceServersReady() fetches /ice, populates engine.rtcConfig.iceServers, and caches the result across peer handshakes.
Scenario 2 (hasTurnConfigured Relay Settle Logic): Verify that when a turn: server is present in rtcConfig.iceServers, _waitForIceGatheringComplete waits for typ relay (or the 1200ms srflx fallback) and completes the full 3-peer encrypted mesh scenario cleanly.
Gate 3: Production Deployment Guide (wrangler secret put)
Document the 2-command Cloudflare TURN activation steps in openspec/specs/overview.md so the operator can enable live 4G/5G TURN relaying on Cloudflare at any time:

wrangler secret put CF_TURN_KEY_ID

wrangler secret put CF_TURN_API_TOKEN
