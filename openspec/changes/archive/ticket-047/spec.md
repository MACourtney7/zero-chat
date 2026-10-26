# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-047"
title: "Browser Runtime Integration of WasmCryptoSession & Blind Dead-Drop"
state: "COMPLETED"
created_at: "2026-10-26T19:50:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-047/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

Following the delivery of `ticket-046`, `zero_protocol` and `zero_client` provide a production-ready Rust/WASM cryptographic engine (`WasmCryptoSession`) implementing the Option B Modernized Burner Keybundle (`ed25519-dalek` signing + `x25519-dalek` Diffie-Hellman key agreement + `chacha20poly1305` AEAD encryption + blind dead-drop SDP sealing + `ZeroizeOnDrop` memory wiping).

However, as diagnosed in [`docs/HANDOVER_TICKET_047_BROWSER_WASM_CRYPTO_INTEGRATION.md`](file:///Users/mattcourtney/Library/CloudStorage/GoogleDrive-mattadam.courtney@gmail.com/My%20Drive/zero_chat/docs/HANDOVER_TICKET_047_BROWSER_WASM_CRYPTO_INTEGRATION.md), [`zero_client/index.html`](file:///Users/mattcourtney/Library/CloudStorage/GoogleDrive-mattadam.courtney@gmail.com/My%20Drive/zero_chat/zero_client/index.html) still relies on legacy JavaScript stubs:
1. `crypto.getRandomValues` generates dummy 32-byte byte arrays for public keys.
2. `PeerJoined` introductions carry `public_key: new Array(32).fill(0)` without Ed25519 signatures.
3. `DropPayload` transmits the joiner's WebRTC `sdp_answer.sdp` in cleartext JSON through the Cloudflare Worker.
4. `RelaySdpOffer`, `RelaySdpAnswer`, and `BroadcastChat` encode cleartext strings directly via `new TextEncoder().encode()` inside `SealedFrame.ciphertext` without pairwise AEAD encryption or cryptographic signatures.

`ticket-047` bridges `WasmCryptoSession` into [`zero_client/index.html`](file:///Users/mattcourtney/Library/CloudStorage/GoogleDrive-mattadam.courtney@gmail.com/My%20Drive/zero_chat/zero_client/index.html), eliminating 100% of the JavaScript cryptographic stubs while strictly preserving all UI, mobile visualViewport, clipboard, and 12-second disconnect grace-timer mechanics established through `ticket-045`.

---

## 2. Requirements & Acceptance Criteria

### REQ-047.1: WASM Module Initialization & Session Lifecycle
- In [`zero_client/index.html`](file:///Users/mattcourtney/Library/CloudStorage/GoogleDrive-mattadam.courtney@gmail.com/My%20Drive/zero_chat/zero_client/index.html):
  - Provide an async readiness singleton `ensureCryptoReady()`:
    - In browser environments, imports `./pkg/zero_client.js` (`initWasm`, `WasmCryptoSession`).
    - In test/mock environments, utilizes `globalThis.WasmCryptoSession` and `globalThis.initWasm`.
    - Initializes a singleton `WasmCryptoSession` instance in `cryptoSession`.
  - Maintain an in-memory map of verified remote peer public keys:
    ```javascript
    const peerPublicKeys = new Map(); // peerId -> { x25519: number[], ed25519: number[] | null }
    ```
  - **Session Teardown & Volatile Zeroization**:
    - In `handlePeerDeparture(peerId)`: call `cryptoSession?.remove_peer(peerId)` and `peerPublicKeys.delete(peerId)`.
    - In `resetToHome()`: call `cryptoSession?.burn_all()`, reinitialize `cryptoSession = new WasmCryptoSession()`, and `peerPublicKeys.clear()`.

### REQ-047.2: Real Burner Keys & Blind Dead-Drop Handshake
- **Host Invite Creation** (`createRoomAsHost` & `handleHeaderAddPeerClick`):
  - Acquire `crypto = await ensureCryptoReady()`.
  - Export real burner public keys: `hostX25519Pub = Array.from(crypto.x25519_public_key_bytes())` and `hostEd25519Pub = Array.from(crypto.ed25519_public_key_bytes())`.
  - Compute invite HMAC token via `crypto.compute_hmac_hex(hostSecretBytes, inviteUuid)`.
  - Embed `host_public_key: hostX25519Pub` and `host_signing_key: hostEd25519Pub` inside `invitePayload` (encoded in URL hash `#payload=`).
- **Joiner Blind Dead-Drop** (`joinRoomFromHash`):
  - Acquire `crypto = await ensureCryptoReady()`.
  - Extract `hostX25519Bytes` and `hostEd25519Bytes` from invite payload; record in `peerPublicKeys.set(hostPeerId, ...)`.
  - Seal WebRTC SDP answer using `crypto.seal_drop_sdp(...)`, returning `sealed_sdp_frame`.
  - Dispatch `dropPayload` with `sdp_answer: { sdp: "SEALED" }`, `sealed_sdp_answer: sealed_sdp_frame`, `joiner_public_key: Array.from(crypto.x25519_public_key_bytes())`, and `joiner_signing_key: Array.from(crypto.ed25519_public_key_bytes())`.
- **Host Dead-Drop Decryption** (`hostWs.onmessage` & `pendingInviteWs.onmessage`):
  - Extract joiner keys and record in `peerPublicKeys.set(targetPeerId, ...)`.
  - If `drop.sealed_sdp_answer` is present, decrypt using `crypto.open_drop_sdp(...)` to recover the plaintext SDP string.
  - Fallback: if `drop.sealed_sdp_answer` is absent (legacy test payloads), register peer via `crypto.register_peer(...)` and use `drop.sdp_answer.sdp`.

### REQ-047.3: Signed Multi-Peer Mesh Introductions (`PeerJoined`)
- **Propagation of Real Public Keys**:
  - When an introducer relays `PeerJoined` to `peerId` introducing `otherId`, retrieve `peerPublicKeys.get(otherId)` and populate `public_key: otherKeys.x25519` and `signing_key: otherKeys.ed25519`.
  - Newly connected peers send an authenticated self-introduction (`PeerJoined` carrying own keys and Ed25519 signature over `(localPeerId || x25519_pub)`).
- **Registration on Receipt**:
  - On receiving `PeerJoined`:
    - Record keys in `peerPublicKeys.set(peer_id, ...)`.
    - If `!cryptoSession.has_peer_session(peer_id)`, call `cryptoSession.register_peer(...)` prior to `initiate_relay_handshake(peer_id)` so the pairwise key is ready to seal `RelaySdpOffer`.

### REQ-047.4: Per-Peer AEAD Sealing & Opening (`RelaySdpOffer`, `RelaySdpAnswer`, `BroadcastChat`)
- **Relay SDP Signaling**:
  - In `initiate_relay_handshake(remotePeerId)`: seal offer SDP for `remotePeerId` using `cryptoSession.seal_for_peer(remotePeerId, offerBytes)`.
  - Receiving node opens `sealed_offer` with `cryptoSession.open_from_peer(fromPeerId, ...)` and seals answer back with `cryptoSession.seal_for_peer(fromPeerId, answerBytes)`.
  - Intermediate relay hops (`to_peer_id !== myPeerId`) decrement TTL and forward `sealed_offer`/`sealed_answer` untouched without attempting decryption.
- **Per-Peer Chat Broadcast (`doSendMessage` & `BroadcastChat`)**:
  - In `doSendMessage`: iterate over all open peers in `engine.get_connected_peer_ids()`, sealing message text specifically for each `peerId` using `cryptoSession.seal_for_peer(peerId, plaintextBytes)`.
  - Send the envelope containing that peer's `sealed_payload` directly over its DataChannel.
  - On receipt of `BroadcastChat`: decrypt and verify Ed25519 signature + Poly1305 tag via `cryptoSession.open_from_peer(senderPeerId, ...)`.
  - If decryption or verification throws (tampered ciphertext, invalid MAC, forged signature), drop the frame and log a security warning without rendering to the DOM.

### REQ-047.5: Automated Verification & Test 22
- In `zero_client/tests/browser/test_mesh_scenario.js` and `zero_client/tests/browser/mock_environment.js`:
  - Implement mock/real `WasmCryptoSession` test bridge.
  - Add Test 22:
    - **No Cleartext Wire Leaks**: Assert `DropPayload.sdp_answer.sdp === "SEALED"` and `sealed_sdp_answer.ciphertext` contains no plaintext SDP or IP strings.
    - **Genuine AEAD Ciphertext**: Assert `BroadcastChat.sealed_payload.ciphertext` does not match message text bytes and carries 12-byte nonce, 16-byte Poly1305 tag, and 64-byte Ed25519 signature.
    - **3-Peer Full Mesh & Host Departure Continuity**: Complete encrypted chat roundtrip across 3 peers and verify encrypted pairwise messaging persists after host leaves.
    - **Tamper Rejection**: Verify that mutating ciphertext or signature in an incoming `BroadcastChat` frame causes `open_from_peer` failure and suppresses DOM rendering.
- Quality Gates:
  - `cargo test --workspace` (136 tests passing).
  - Dual WASM check (`zero_client`, `zero_network`).
  - Formatting & linting (`cargo fmt -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`).
  - All 22 browser headless regression tests passing cleanly (`bash zero_client/tests/browser/run_test.sh`).

---

## 3. Target File Scope & Blast Radius

| File | Subsystem | Planned Change |
| :--- | :--- | :--- |
| `zero_client/index.html` | Client UI & WebRTC Runtime | Bridge `WasmCryptoSession`, eliminate JS stubs, blind dead-drop SDP, signed mesh introductions, per-peer sealed broadcast |
| `zero_client/tests/browser/mock_environment.js` | Test Infrastructure | Provide `WasmCryptoSession` and `initWasm` test bridge in headless JSC environment |
| `zero_client/tests/browser/test_mesh_scenario.js` | Browser Test Suite | Implement Test 22 (no cleartext wire leaks, genuine AEAD, tamper rejection, host departure encrypted continuity) |

---

## 4. Subagent Brief Decomposition & Sizing

| Brief | Cognitive Sizing | Scope & Role | Target Files |
| :--- | :---: | :--- | :--- |
| `brief-impl-047-runtime.md` | `MEDIUM` [M] | Bridge `WasmCryptoSession` into `zero_client/index.html`: blind dead-drop, signed introductions, per-peer sealed messaging | `zero_client/index.html` |
| `brief-impl-047-tests.md` | `MEDIUM` [M] | Implement `mock_environment.js` crypto bridge and Test 22 in `test_mesh_scenario.js` | `zero_client/tests/browser/**` |
| `brief-validation.md` | `MEDIUM` [M] | Comprehensive quality gates: `cargo test`, dual wasm compilation, clippy, dry-run, browser regression | Workspace-wide |
| `brief-review-1.md` | `HIGH` [H] | Independent high-scrutiny code review: zero cleartext leaks, signature verification, non-degradation of UI/viewport/grace timer | All modified files |

---

## 5. Security & Invariant Safeguards

1. **Non-Degradation of Mobile & UI Mechanics**:
   - Layout, styling, `.visualViewport` handlers, `peerRoles`, and 12-second `pendingDisconnectTimers` remain completely untouched.
2. **Zero-Knowledge Ephemerality**:
   - Memory is zeroized on peer departure (`remove_peer`) and room exit (`burn_all()`).
3. **Fail-Closed Authenticated Envelopes**:
   - Any signature tampering or Poly1305 MAC failure immediately aborts frame processing without DOM mutation.
