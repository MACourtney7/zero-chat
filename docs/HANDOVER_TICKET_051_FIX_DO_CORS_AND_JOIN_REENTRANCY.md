Architectural Handover Brief: Fix Durable Object Dead-Drop Immutable Header Crash (net::ERR_FAILED / Missing CORS) & Joiner Failure Re-Entrancy (ticket-051)
1. Root Cause Analysis (From Live Production Console Trace)
Live browser console logs from the Pages client and Worker endpoint revealed the exact failure chain when a Joiner opens an invite link:

Access to fetch at 'https://<worker-host>/drop/<do_id>/<invite_uuid>'

from origin 'https://<pages-host>' has been blocked by CORS policy:

No 'Access-Control-Allow-Origin' header is present on the requested resource.

Failed to load resource: net::ERR_FAILED

Joiner failed: Error: Connection closed before opening

Joiner failed: TypeError: Failed to fetch
Root Cause 1: Cloudflare Workers Can't modify immutable headers Exception on Durable Object Subrequests (zero_network/src/lib.rs)
THIS IS THE SMOKING GUN: Look at what happened across the two browser windows simultaneously:

On the Left Window (Host), the UI progressed from "Invite link copied to clipboard" -> "Waiting for peer..." -> "Establishing secure connection..".
Why did the Host window print "Establishing secure connection.."? Because the Durable Object (DeadDropRoom) received the Joiner's POST /drop/:do_id/:invite_uuid, verified the HMAC token, burned the UUID, and successfully piped the DropPayload down the Host's WebSocket!
Then, the Durable Object returned its worker::Response (200 OK) back to the outer Worker in zero_network/src/lib.rs:

let resp = stub.fetch_with_request(req).await?;

handlers::with_cors(resp)

In Cloudflare's Workers runtime (worker-rs), a Response returned across an RPC/subrequest boundary from stub.fetch_with_request(req).await has immutable headers in V8 (Can't modify immutable headers)!
When zero_network/src/lib.rs called handlers::with_cors(resp) on the response returned by stub.fetch_with_request(req).await, resp.with_headers(headers) threw a JS TypeError: Can't modify immutable headers exception inside the Worker!
Because the Worker threw an unhandled exception after the Durable Object had already pushed the DropPayload to the Host's WebSocket:
Cloudflare killed the Joiner's HTTP request with a bare 500 / net::ERR_FAILED response lacking Access-Control-Allow-Origin: *.
The Joiner's fetch() threw TypeError: Failed to fetch, which triggered handleJoinFailure() and closed the Joiner's RTCPeerConnection while the Host was trying to complete the WebRTC handshake!
Root Cause 2: Re-Entrant handleJoinFailure() in zero_client/index.html
When fetch(...) threw TypeError: Failed to fetch, joinRoomFromHash called handleJoinFailure(err). Inside handleJoinFailure(err):

engine.close() was called before isJoining = false.
engine.close() synchronously invoked onStateChangeCb("closed"), which checked if (isJoining) (still true!) and called handleJoinFailure(new Error("Connection closed before opening")) a second time—printing "Failed to establish connection" twice!


2. Target File Scope
zero_network/src/lib.rs & zero_network/src/handlers.rs & zero_network/src/durable_object.rs
zero_client/index.html
scripts/test_mesh_scenario.js


3. Detailed Technical Specification
Part A: Reconstruct Mutable worker::Response After stub.fetch_with_request in zero_network
Never Call .with_headers(...) Directly on a Subrequest Response Returned from stub.fetch_with_request (zero_network/src/lib.rs & handlers.rs):
In zero_network/src/lib.rs (and any helper in handlers.rs), when stub.fetch_with_request(req).await returns a mut resp:
Case 1 (resp.status_code() == 101 WebSocket Upgrade): Return Ok(resp) immediately without calling with_cors(resp)! Calling with_cors on a 101 Switching Protocols WebSocket response either throws Can't modify immutable headers or strips the attached webSocket handle.
Case 2 (resp.status_code() != 101 HTTP Response from Durable Object, such as /drop or /register): Extract the status code and body text from resp, construct a brand-new mutable worker::Response in the outer Worker isolate, and apply with_cors to that fresh response:

let mut resp = match stub.fetch_with_request(req).await {

    Ok(r) => r,

    Err(e) => {

        let err_resp = Response::error(format!("Durable Object error: {e}"), 500)?;

        return handlers::with_cors(err_resp);

    }

};

if resp.status_code() == 101 {

    return Ok(resp);

}

let status = resp.status_code();

let body_text = resp.text().await.unwrap_or_default();

let fresh_resp = if (200..300).contains(&status) {

    let mut r = Response::ok(body_text)?;

    if status != 200 {

        r = r.with_status(status);

    }

    r

} else {

    Response::error(body_text, status)?

};

handlers::with_cors(fresh_resp)

Update with_cors and cors_preflight_response in zero_network/src/handlers.rs:
In with_cors(resp: worker::Response), if resp.status_code() == 101, return Ok(resp) immediately as a defensive guard.
Ensure with_cors sets:
Access-Control-Allow-Origin: *
Access-Control-Allow-Methods: GET, POST, OPTIONS
Access-Control-Allow-Headers: Content-Type, Authorization, X-Host-Secret
Access-Control-Max-Age: 86400
And if status is 200 or 201 and Content-Type is not set, set Content-Type: application/json.
Part B: Fix handleJoinFailure Re-Entrancy & Host crypto Shadowing (zero_client/index.html)
Prevent Re-Entrant Double Execution in handleJoinFailure(err):
In zero_client/index.html, check and clear isJoining at the very top of handleJoinFailure(err) before calling engine.close():

function handleJoinFailure(err) {

  if (!isJoining) return;

  isJoining = false;

  console.error("Joiner failed:", err);

  if (joinTimeoutHandle) {

    clearTimeout(joinTimeoutHandle);

    joinTimeoutHandle = null;

  }

  if (engine) {

    engine.close();

  }

  clearTransientStatus();

  appendSystemMessage("Failed to establish connection");

  showToast("Invite link expired or invalid");

  if (window.history && window.history.replaceState) {

    window.history.replaceState(null, "", window.location.pathname + window.location.search);

  }

  setActionButtonMode("create-new");

}

Fix const crypto Shadowing in createRoomAsHost() (zero_client/index.html):
Rename const crypto = await ensureCryptoReady(); inside createRoomAsHost() to const wasmCrypto = await ensureCryptoReady(); so it does not shadow the global window.crypto object.


4. Verification & Acceptance Criteria
cargo test --workspace — All Rust workspace tests pass.
cargo clippy --workspace --all-targets -- -D warnings — Zero warnings.
./scripts/deploy_dry_run.sh — Builds release WASM bundle cleanly.
node scripts/test_mesh_scenario.js — Add a test verifying handleJoinFailure does not double-fire when engine.close() emits "closed", and verify all headless E2E scenarios pass.
