# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-053"
title: "Migrate Cloudflare Workers Subdomain from zero-chat to zero-network"
state: "IN_PROGRESS"
created_at: "2026-11-01T10:27:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-053/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

The user is migrating their Cloudflare Workers subdomain from `zero-chat` to `zero-network`.
Cloudflare Pages remains untouched (`zero-chat.pages.dev` and project name `zero-chat` in `.github/workflows/deploy.yml` remain unchanged).

In the client codebase, default Worker endpoints point to the current `workers.dev` subdomain:
- `WORKER_ORIGIN`: `"https://network.zero-chat.workers.dev"`
- `WORKER_WS_ORIGIN`: `"wss://network.zero-chat.workers.dev"`

When the Cloudflare `workers.dev` subdomain is updated to `zero-network`, the worker named `network` will resolve at:
- `https://network.zero-network.workers.dev`
- `wss://network.zero-network.workers.dev`

Without updating these origins, the client will fail to connect to signaling or obtain TURN credentials (`net::ERR_NAME_NOT_RESOLVED`).

---

## 2. Requirements & Acceptance Criteria

### REQ-053.1: Update Default Worker Endpoints in Client
- In `zero_client/index.html`:
  - `const WORKER_ORIGIN = "https://network.zero-network.workers.dev";`
  - `const WORKER_WS_ORIGIN = "wss://network.zero-network.workers.dev";`
- Maintain fallback support for `window.WORKER_HTTP_ORIGIN` for local or staging overrides.

### REQ-053.2: Update Headless Browser Test Scenarios
- In `zero_client/tests/browser/test_mesh_scenario.js`:
  - Replace occurrences of `https://network.zero-chat.workers.dev` with `https://network.zero-network.workers.dev` across test scenarios (Scenarios 12, 16, 22).

### REQ-053.3: Update Deployment Documentation
- In `docs/deployment.md`:
  - Update Worker routes documentation to `https://network.zero-network.workers.dev`.

### REQ-053.4: Cloudflare Pages Boundaries (Strict Non-Modification)
- Cloudflare Pages project name `zero-chat` in `.github/workflows/deploy.yml` and branding title `zero-chat.pages.dev` in `zero_client/index.html` are strictly preserved and unchanged.

### REQ-053.5: Quality Gates & Verification
- Execute `cargo test --workspace` (all 122+ tests pass).
- Execute `cargo clippy --workspace --all-targets -- -D warnings` (clean).
- Execute `cargo fmt -- --check` (clean).
- Execute `./scripts/deploy_dry_run.sh` (clean).
- Execute `bash zero_client/tests/browser/run_test.sh` (all 24 browser tests pass).

---

## 3. Blast Radius & Affected Files

- `zero_client/index.html` (Lines 201, 203)
- `zero_client/tests/browser/test_mesh_scenario.js` (Lines 799, 1294, 1439, 2520)
- `docs/deployment.md` (Line 77)

---

## 4. Work Breakdown (Brief Decomposition)

- **Brief 1 (Implementer)**:
  - Update `zero_client/index.html` lines 201 & 203 to `https://network.zero-network.workers.dev` and `wss://network.zero-network.workers.dev`.
  - Update `zero_client/tests/browser/test_mesh_scenario.js` test drop URLs to `https://network.zero-network.workers.dev`.
  - Update `docs/deployment.md` to reference `https://network.zero-network.workers.dev`.
- **Brief 2 (Validator)**:
  - Run all quality gates: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt -- --check`, `./scripts/deploy_dry_run.sh`, and `bash zero_client/tests/browser/run_test.sh`.
- **Brief 3 (Reviewer)**:
  - Conduct independent review verifying strict boundary enforcement (Pages unchanged) and correct worker origin routing.

---

## 5. Verification Plan

1. Verify string references:
   `grep -rn "zero-chat.workers.dev" zero_client docs` returns 0 results.
2. Full quality gate execution:
   `cargo test --workspace`
   `cargo clippy --workspace --all-targets -- -D warnings`
   `cargo fmt -- --check`
   `./scripts/deploy_dry_run.sh`
   `bash zero_client/tests/browser/run_test.sh`
