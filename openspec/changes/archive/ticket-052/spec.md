# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-052"
title: "Fix Durable Object Dead-Drop Immutable Header Crash & Joiner Failure Re-Entrancy"
state: "COMPLETED"
created_at: "2026-10-28T17:10:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-052/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

Live production console traces when a Joiner opens an invite link revealed a critical CORS failure and duplicate error notice:
```text
Access to fetch at 'https://<worker-host>/drop/<do_id>/<invite_uuid>' from origin 'https://<pages-host>' has been blocked by CORS policy: No 'Access-Control-Allow-Origin' header is present on the requested resource.
Failed to load resource: net::ERR_FAILED
Joiner failed: Error: Connection closed before opening
Joiner failed: TypeError: Failed to fetch
```

### Root Cause 1: Immutable Headers on Durable Object Subrequest Responses (`zero_network/src/lib.rs`)
In Cloudflare Workers (`worker-rs`), responses returned across an RPC / subrequest boundary from `stub.fetch_with_request(req).await` have immutable headers in V8 (`guard: "immutable"`). In `zero_network/src/lib.rs`, the router called `handlers::with_cors(resp)` directly on the response returned from `stub.fetch_with_request(req).await`. Calling `resp.headers_mut().set(...)` on an immutable response threw an unhandled JavaScript `TypeError: Can't modify immutable headers` exception inside the Worker.
Because the Durable Object had already verified the HMAC, burned the UUID, and piped the `DropPayload` to the Host's WebSocket before the exception occurred, the Host progressed to `"Establishing secure connection..."`, while Cloudflare killed the Joiner's HTTP response with a bare 500 without CORS headers (`net::ERR_FAILED`).

### Root Cause 2: Re-Entrant `handleJoinFailure()` in `zero_client/index.html`
When the Joiner's `fetch()` threw `TypeError: Failed to fetch`, `joinRoomFromHash` called `handleJoinFailure(err)`. Inside `handleJoinFailure(err)`, `engine.close()` was invoked before `isJoining` was reset to `false`. `engine.close()` synchronously fired the `onStateChangeCb("closed")` callback, which observed `if (isJoining)` as still `true` and invoked `handleJoinFailure(new Error("Connection closed before opening"))` re-entrantly. This resulted in duplicate system messages and console error spam.

### Root Cause 3: `window.crypto` Shadowing
In `createRoomAsHost()` in `zero_client/index.html`, `const crypto = await ensureCryptoReady();` shadowed `window.crypto`.

`ticket-052` addresses these issues across `zero_network` and `zero_client`.

---

## 2. Requirements & Acceptance Criteria

### REQ-052.1: Reconstruct Fresh Mutable Response in `zero_network/src/lib.rs`
- In `zero_network/src/lib.rs`, for `/drop/:do_id/:invite_uuid`:
  - When `stub.fetch_with_request(req).await` returns:
    - If `status == 101` (WebSocket upgrade), return `Ok(resp)` directly without invoking `with_cors`.
    - If `status != 101`:
      - Extract `status` and `body_text = resp.text().await.unwrap_or_default()`.
      - Construct a brand-new mutable `Response` in the outer isolate:
        - If `(200..300).contains(&status)`: `Response::ok(body_text)` (with status adjusted if != 200).
        - Else: `Response::error(body_text, status)`.
      - Pass the fresh mutable response to `handlers::with_cors(...)`.
    - If `stub.fetch_with_request` returns `Err(e)`:
      - Construct `Response::error(format!("Durable Object error: {e}"), 500)` and pass to `with_cors`.

### REQ-052.2: Defensive 101 Guard & CORS Headers in `handlers::with_cors`
- In `zero_network/src/handlers.rs`:
  - In `with_cors(mut resp: Response)`:
    - If `resp.status_code() == 101`, return `Ok(resp)` immediately.
    - Set `Access-Control-Allow-Origin: *`.
    - Set `Access-Control-Allow-Methods: GET, POST, OPTIONS`.
    - Set `Access-Control-Allow-Headers: Content-Type, Authorization, X-Host-Secret`.
    - Set `Access-Control-Max-Age: 86400`.
    - If status is `200` or `201` and `Content-Type` is not already present, set `Content-Type: application/json`.

### REQ-052.3: Prevent Re-Entrant Execution in `handleJoinFailure()`
- In `zero_client/index.html`:
  - At the very top of `handleJoinFailure(err)`:
    ```javascript
    if (!isJoining) return;
    isJoining = false;
    ```
  - Clean up timers, close engine, update status, and restore UI cleanly without double-execution.

### REQ-052.4: Eliminate `const crypto` Shadowing
- In `zero_client/index.html`:
  - Rename `const crypto = await ensureCryptoReady();` in `createRoomAsHost()` to `const wasmCrypto = await ensureCryptoReady();`.

### REQ-052.5: Browser E2E Headless Verification
- In `zero_client/tests/browser/test_mesh_scenario.js`:
  - Add Test 24: Assert that when a join failure occurs, `handleJoinFailure` executes exactly once, `isJoining` is cleared before `engine.close()`, and only a single `"Failed to establish connection"` notice is rendered.
  - Verify all 24 browser scenarios pass.

### REQ-052.6: Quality Gates & Zero Regressions
- Pass `cargo test --workspace` (117+ tests).
- Pass `cargo clippy --workspace --all-targets -- -D warnings`.
- Pass `cargo fmt -- --check`.
- Pass `./scripts/deploy_dry_run.sh`.
- Pass `bash zero_client/tests/browser/run_test.sh`.

---

## 3. Architecture & Affected Modules

- `zero_network/src/lib.rs`: Fresh response construction after DO subrequest.
- `zero_network/src/handlers.rs`: Guard against 101 status and add `Max-Age` / `Content-Type` headers.
- `zero_client/index.html`: Re-entrancy guard in `handleJoinFailure` and variable de-shadowing.
- `zero_client/tests/browser/test_mesh_scenario.js`: Test 24 verifying join failure non-re-entrancy.
