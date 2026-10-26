Architectural Handover Brief: Browser Runtime Integration of WasmCryptoSession & Blind Dead-Drop (ticket-047)
1. Executive Summary & Objective
With ticket-046 complete, zero_protocol and zero_client now provide a fully tested Rust/WASM cryptographic engine (WasmCryptoSession) implementing the Option B Modernized Burner Keybundle (ed25519-dalek signing + x25519-dalek Diffie-Hellman key agreement + chacha20poly1305 AEAD encryption + blind dead-drop SDP sealing + ZeroizeOnDrop memory wiping).

However, zero_client/index.html still uses legacy JavaScript stubs (crypto.getRandomValues placeholder keys, new Array(32).fill(0) in PeerJoined introductions, cleartext sdp_answer in DropPayload, and unencrypted TextEncoder / TextDecoder byte arrays inside SealedFrame).

ticket-047 bridges WasmCryptoSession from ./pkg/zero_client.js directly into zero_client/index.html, eliminating 100% of the JavaScript cryptographic stubs while strictly preserving all UI, mobile visualViewport, clipboard, and 12-second disconnect grace-timer mechanics established through ticket-045.


2. Target File Scope
zero_client/index.html (Import and initialize ./pkg/zero_client.js & WasmCryptoSession, replace all stubbed keys/frames with real WASM crypto calls, seal/open blind dead-drop SDP answers, propagate and verify peer public keys across PeerJoined mesh introductions, and invoke burn_all() on session teardown)
scripts/test_mesh_scenario.js or Playwright E2E test suite (Ensure headless browser tests initialize the WASM module and verify genuine encrypted SealedFrame payloads and blind dead-drop handshakes)

Critical Invariant: Do not alter the DOM layout, CSS classes, mobile visualViewport listeners, clipboard fallback logic, peerRoles tracking, or pendingDisconnectTimers grace period in zero_client/index.html. Only replace the cryptographic key generation, dead-drop SDP sealing, mesh key introduction, and frame sealing/opening logic.


3. Detailed Technical Specification
Part A: WASM Module Initialization & Session Lifecycle (zero_client/index.html)
Import WASM Bindings: At the top of the <script type="module"> block in zero_client/index.html, import the generated WASM initializer and WasmCryptoSession:

import initWasm, { WasmCryptoSession } from "./pkg/zero_client.js";

Initialize Before Room Actions: Create a singleton async readiness helper ensureCryptoReady():

let wasmInitialized = false;

let cryptoSession = null;

async function ensureCryptoReady() {

  if (!wasmInitialized) {

    await initWasm();

    wasmInitialized = true;

  }

  if (!cryptoSession) {

    cryptoSession = new WasmCryptoSession();

  }

  return cryptoSession;

}

Call await ensureCryptoReady() at application startup and at the start of createRoomAsHost(), handleHeaderAddPeerClick(), and joinRoomFromHash().

Track Remote Peer Public Keys for Mesh Introductions: Maintain an in-memory map of verified peer public keys so that when a node introduces two existing peers (PeerJoined), it includes their real X25519 and Ed25519 public keys instead of new Array(32).fill(0):

const peerPublicKeys = new Map(); // peerId -> { x25519: number[], ed25519: number[] }

Zeroize WASM Memory on Burn / Reset (resetToHome & handlePeerDeparture):
In handlePeerDeparture(peerId): call cryptoSession?.remove_peer(peerId) and peerPublicKeys.delete(peerId).
In resetToHome(): call cryptoSession?.burn_all(), set cryptoSession = new WasmCryptoSession() (so a fresh burner identity is ready for the next session), and call peerPublicKeys.clear().


Part B: Real Burner Keys & Blind Dead-Drop Handshake
Host Invite Creation (createRoomAsHost & handleHeaderAddPeerClick):

Call const crypto = await ensureCryptoReady();
Replace the random dummy hostPubKey with the real WASM burner keys:

const hostX25519Pub = Array.from(crypto.x25519_public_key_bytes());

const hostEd25519Pub = Array.from(crypto.ed25519_public_key_bytes());

Compute the invite HMAC token via WasmCryptoSession.compute_hmac_hex(hostSecretBytes, inviteUuid).
Include both host_public_key: hostX25519Pub and host_signing_key: hostEd25519Pub inside invitePayload (packed into #payload=).

Joiner Blind Dead-Drop (joinRoomFromHash):

Call const crypto = await ensureCryptoReady();
Extract hostX25519Bytes = new Uint8Array(invite.host_public_key) and hostEd25519Bytes = invite.host_signing_key ? new Uint8Array(invite.host_signing_key) : undefined.
Record the host's public keys in peerPublicKeys.set(hostPeerId, { x25519: invite.host_public_key, ed25519: invite.host_signing_key || null }).
Seal the joiner's WebRTC SDP answer using WasmCryptoSession.seal_drop_sdp:

const sealedSdpJson = crypto.seal_drop_sdp(

  hostPeerId,

  hostX25519Bytes,

  hostEd25519Bytes,

  answerSdp

);

const sealedSdpFrame = JSON.parse(sealedSdpJson);

Construct dropPayload with a blinded cleartext field and the encrypted sealed_sdp_answer:

const joinerX25519Pub = Array.from(crypto.x25519_public_key_bytes());

const joinerEd25519Pub = Array.from(crypto.ed25519_public_key_bytes());

const dropPayload = {

  invite_uuid: invite.invite_uuid,

  token: invite.token,

  sdp_answer: { sdp: "SEALED" },

  sealed_sdp_answer: sealedSdpFrame,

  joiner_public_key: joinerX25519Pub,

  joiner_signing_key: joinerEd25519Pub,

  joiner_peer_id: localPeerId

};

Host Dead-Drop Decryption (hostWs.onmessage & pendingInviteWs.onmessage):

When the Host receives the drop JSON over its WebSocket:
Extract joinerX25519Bytes = new Uint8Array(drop.joiner_public_key) and joinerEd25519Bytes = drop.joiner_signing_key ? new Uint8Array(drop.joiner_signing_key) : undefined.
Save peerPublicKeys.set(targetPeerId, { x25519: drop.joiner_public_key, ed25519: drop.joiner_signing_key || null }).
If drop.sealed_sdp_answer is present, decrypt and authenticate the SDP answer via WASM:

const answerSdp = crypto.open_drop_sdp(

  targetPeerId,

  joinerX25519Bytes,

  joinerEd25519Bytes,

  JSON.stringify(drop.sealed_sdp_answer)

);

await engine.accept_peer_answer(targetPeerId, answerSdp);

(Fallback: if drop.sealed_sdp_answer is absent for legacy test payloads, register the peer via crypto.register_peer(targetPeerId, joinerX25519Bytes, joinerEd25519Bytes) and use drop.sdp_answer.sdp.)


Part C: Signed Multi-Peer Mesh Introductions (PeerJoined)
Include Real Public Keys in PeerJoined Frames:

When an introducer sends PeerJoined to peerId announcing otherId:
Look up const otherKeys = peerPublicKeys.get(otherId).
Populate public_key: otherKeys.x25519 and signing_key: otherKeys.ed25519 inside the PeerJoined signal.
Also allow a newly connected peer to send a self-introduction (PeerJoined with its own localPeerId, x25519_public_key_bytes(), ed25519_public_key_bytes(), and Ed25519 signature over (localPeerId || x25519_pub)) so every direct peer always has authoritative public keys.

Register Pairwise Crypto Session Upon Receiving PeerJoined:

When receiving PeerJoined { peer_id, public_key, signing_key, signature }:
Verify public_key is a valid 32-byte non-zero array.
Store peerPublicKeys.set(peer_id, { x25519: public_key, ed25519: signing_key || null }).
If !cryptoSession.has_peer_session(peer_id), call:

cryptoSession.register_peer(

  peer_id,

  new Uint8Array(public_key),

  signing_key ? new Uint8Array(signing_key) : undefined

);

Crucial Ordering: Ensure cryptoSession.register_peer(...) executes before initiate_relay_handshake(peer_id) so that the pairwise X25519 key is ready to seal RelaySdpOffer!


Part D: Per-Peer AEAD Sealing & Opening (RelaySdpOffer, RelaySdpAnswer, BroadcastChat)
Sealing RelaySdpOffer and RelaySdpAnswer:

In initiate_relay_handshake(remotePeerId):
Seal the offer SDP specifically for remotePeerId:

const offerBytes = new TextEncoder().encode(offerSdp);

const sealedOffer = JSON.parse(cryptoSession.seal_for_peer(remotePeerId, offerBytes));

When receiving RelaySdpOffer addressed to localPeerId (to_peer_id === myPeerId):
Open the sealed offer using from_peer_id's session:

const offerPlainBytes = cryptoSession.open_from_peer(fromPeerId, JSON.stringify(offerPayload.sealed_offer));

const offerSdp = new TextDecoder().decode(offerPlainBytes);

Seal the resulting answer SDP back for fromPeerId:

const answerBytes = new TextEncoder().encode(answerSdp);

const sealedAnswer = JSON.parse(cryptoSession.seal_for_peer(fromPeerId, answerBytes));

When receiving RelaySdpAnswer addressed to localPeerId (to_peer_id === myPeerId):
Open the sealed answer using from_peer_id's session:

const answerPlainBytes = cryptoSession.open_from_peer(fromPeerId, JSON.stringify(ansPayload.sealed_answer));

const answerSdp = new TextDecoder().decode(answerPlainBytes);

Note on Intermediate Relay Hops: When a node is acting as an intermediate relay (to_peer_id !== myPeerId), it cannot and must not decrypt sealed_offer or sealed_answer (as they are encrypted end-to-end between from_peer_id and to_peer_id). It simply decrements ttl and forwards the opaque SealedFrame untouched!

Per-Peer Sealed Chat Broadcast (doSendMessage & BroadcastChat):

Because each peer pair $(A, B)$ and $(A, C)$ has its own distinct pairwise X25519 shared secret in WasmCryptoSession, when sending a chat message to connected peers:
Generate a single msgUuid and timestampMs for the message.
Iterate over each open peerId in engine.get_connected_peer_ids():
Seal plaintextBytes specifically for peerId:

const sealedFrame = JSON.parse(cryptoSession.seal_for_peer(peerId, plaintextBytes));

Send the BroadcastChat envelope containing that peer's sealedFrame directly over peerId's DataChannel (engine.send_to_peer(peerId, JSON.stringify(envelope))).
When receiving BroadcastChat:
Identify the sender peer ID (sender_id or the DataChannel's fromPeerId).
Call const plainBytes = cryptoSession.open_from_peer(senderPeerId, JSON.stringify(chatPayload.sealed_payload)) to verify the Ed25519 signature and Poly1305 MAC tag and decrypt the message text.
If open_from_peer throws (tampered ciphertext, invalid MAC, or forged signature), drop the frame and log a security warning without rendering anything to the DOM.


4. Verification & Acceptance Criteria
Gate 1: Workspace Compilation & Rust Test Suite
cargo test --workspace — All unit and integration tests across zero_protocol, zero_client, and zero_network must pass.
./scripts/deploy_dry_run.sh — Must build the WASM package into zero_client/dist/pkg/ and stage zero_client/dist/index.html and zero_client/dist/style.css.
Gate 2: End-to-End Cryptographic Verification in Browser Tests
No Cleartext Wire Leaks: Inspect captured DataChannel and WebSocket frames during a 3-peer session:
Assert DropPayload.sdp_answer.sdp === "SEALED" and DropPayload.sealed_sdp_answer.ciphertext does not contain "v=0" or IP addresses in plaintext.
Assert BroadcastChat.sealed_payload.ciphertext does not match raw UTF-8 bytes of the message text and includes a 12-byte nonce, 16-byte Poly1305 tag overhead, and 64-byte signature.
3-Peer Mesh & Host Departure Regression: Run the full 3-peer mesh scenario (Peer 1 invites Peer 2, Peer 1 invites Peer 3, $B \leftrightarrow C$ snaps via encrypted RelaySdpOffer/RelaySdpAnswer, all 3 peers exchange encrypted chat messages, Peer 1 leaves, and Peer 2 & Peer 3 continue chatting encrypted).
Tamper Rejection Test: Verify that mutating a byte of ciphertext or signature in an incoming BroadcastChat frame causes open_from_peer to reject the frame and prevents DOM rendering.
