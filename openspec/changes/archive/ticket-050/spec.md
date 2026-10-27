# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-050"
title: "Cloudflare TURN API 201 Created and Generate ICE Servers Path Resolution"
state: "COMPLETED"
created_at: "2026-10-27T22:48:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-050/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

In `ticket-049`, zero-knowledge ephemeral TURN credential provisioning was added to `zero_network` via `GET /ice`, checking `CF_TURN_KEY_ID` and `CF_TURN_API_TOKEN` to generate 1-hour credentials from Cloudflare Calls.

However, during live production activation, when the credentials were deployed, `/ice` continued falling back to default STUN due to two Cloudflare Realtime TURN API specifics:
1. **HTTP Status `201 Created` vs `200 OK`**:
   Cloudflare's Calls TURN API returns `201 Created` when generating new credential resources. In `zero_network/src/handlers.rs`, the handler strictly checked `if cf_res.status_code() == 200`. Because `201 != 200`, the check evaluated to `false`, emitting a warning and triggering the fallback to default STUN.
2. **Dedicated WebRTC Endpoint (`generate-ice-servers`)**:
   Cloudflare provides a dedicated endpoint:
   `https://rtc.live.cloudflare.com/v1/turn/keys/{key_id}/credentials/generate-ice-servers`
   which directly returns the standard `{ "iceServers": [...] }` structure expected by WebRTC `RTCPeerConnection`.
3. **Resilient JSON Parsing**:
   `parse_custom_ice_servers` currently handles arrays and objects with an `iceServers` key, but should also handle single flat credential objects containing `"urls"`, wrapping them automatically into `{ "iceServers": [obj] }`.

`ticket-050` resolves these discrepancies so that when `CF_TURN_KEY_ID` and `CF_TURN_API_TOKEN` are configured, the Worker successfully mints and returns Cloudflare Realtime TURN credentials with zero fallbacks.

---

## 2. Requirements & Acceptance Criteria

### REQ-050.1: Target `generate-ice-servers` Endpoint
- In `zero_network/src/handlers.rs` within `handle_ice_config`:
  - Update the Cloudflare Calls TURN request URL to:
    `https://rtc.live.cloudflare.com/v1/turn/keys/{key_id}/credentials/generate-ice-servers`
  - Send the payload `{"ttl": 86400}` (or 3600) with `Authorization: Bearer {api_token}` and `Content-Type: application/json`.

### REQ-050.2: Accept 2xx Status Codes (`201 Created` Support)
- In `zero_network/src/handlers.rs`:
  - Check `let status = cf_res.status_code();`
  - Replace the strict `status == 200` check with `(200..=299).contains(&status)`, explicitly accepting `201 Created` and `200 OK`.

### REQ-050.3: Resilient ICE Server JSON Normalization
- In `zero_network/src/handlers.rs`:
  - Update `parse_custom_ice_servers`:
    - If input is an object with an `iceServers` key, return `{ "iceServers": servers }`.
    - If input is an object containing `"urls"`, wrap it into `{ "iceServers": [object] }`.
    - If input is an array, return `{ "iceServers": array }`.
  - In `handle_ice_config`, parse the Cloudflare response body through `parse_custom_ice_servers` first, falling back to `serde_json::from_str`.

### REQ-050.4: Unit Tests for 201 Response & Flat Credential Objects
- In `zero_network/tests/network_tests.rs`:
  - Add test asserting `parse_custom_ice_servers` correctly wraps a flat credential object `{"urls": "turn:turn.cloudflare.com:3478", "username": "user", "credential": "pwd"}` into `{ "iceServers": [...] }`.
  - Verify default fallback and error handling remain intact.

### REQ-050.5: Quality Gates & Zero Regressions
- Pass `cargo test --workspace` (117+ tests).
- Pass `cargo clippy --workspace --all-targets -- -D warnings`.
- Pass `cargo fmt -- --check`.
- Pass `bash zero_client/tests/browser/run_test.sh` (23 browser tests).

---

## 3. Architecture & Affected Modules

- `zero_network/src/handlers.rs`:
  - `parse_custom_ice_servers` update for flat credential objects.
  - `handle_ice_config` URL path and `(200..=299).contains(&status)` check.
- `zero_network/tests/network_tests.rs`:
  - Add test cases for flat credential objects in `parse_custom_ice_servers`.
