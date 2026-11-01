# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-053"
title: "Migrate Cloudflare Subdomain from zero-chat to zero-network"
state: "PLANNING"
created_at: "2026-11-01T10:27:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-053/main"
max_review_cycles: 3
---
```

## 1. Problem Statement & User Intent

The user wants to migrate their Cloudflare subdomain from `zero-chat` to `zero-network`.
In the `zero_chat` codebase, Cloudflare Workers endpoints are configured for signaling, dead-drop exchanges, and TURN credential acquisition (`/ice`, `/drop`, `/host`).

Currently, the default Worker endpoint origins in `zero_client/index.html` are configured as:
- `WORKER_ORIGIN`: `"https://network.zero-chat.workers.dev"`
- `WORKER_WS_ORIGIN`: `"wss://network.zero-chat.workers.dev"`

When the Cloudflare `workers.dev` subdomain is changed in the Cloudflare dashboard from `zero-chat` to `zero-network`, any traffic pointed at `*.zero-chat.workers.dev` will fail with DNS resolution errors or 404s.

Therefore, the codebase must be updated to align with the new subdomain:
1. Update default origins in `zero_client/index.html` to:
   - `https://network.zero-network.workers.dev`
   - `wss://network.zero-network.workers.dev`
2. Update mock/test signaling drop URLs in `zero_client/tests/browser/test_mesh_scenario.js` to reference `network.zero-network.workers.dev`.
3. Update `docs/deployment.md` to reflect the updated Worker routing (`https://network.zero-network.workers.dev`).
4. Clarify Cloudflare Pages project naming:
   - Cloudflare Pages currently deploys to project `zero-chat` (`zero-chat.pages.dev`).
   - If the user also wants to migrate Cloudflare Pages to `zero-network.pages.dev`, the project name in `.github/workflows/deploy.yml` and branding references in `zero_client/index.html` would be updated accordingly.

---

## 2. Requirements & Acceptance Criteria

### REQ-053.1: Update Default Worker Endpoints in Client
- In `zero_client/index.html`:
  - `const WORKER_ORIGIN = "https://network.zero-network.workers.dev";`
  - `const WORKER_WS_ORIGIN = "wss://network.zero-network.workers.dev";`
- Maintain fallback support for `window.WORKER_HTTP_ORIGIN` for local or staging overrides.

### REQ-053.2: Update Headless Browser Test Scenarios
- In `zero_client/tests/browser/test_mesh_scenario.js`:
  - Replace occurrences of `https://network.zero-chat.workers.dev` with `https://network.zero-network.workers.dev` across all test scenarios (Scenarios 12, 16, 22).

### REQ-053.3: Update Deployment Documentation
- In `docs/deployment.md`:
  - Update Worker routes documentation to `https://network.zero-network.workers.dev`.

### REQ-053.4: Cloudflare Pages Subdomain / Project Alignment (Conditional / Scope Boundary)
- If Cloudflare Pages project is also being migrated:
  - Update `.github/workflows/deploy.yml` pages project name: `zero-network`.
  - Update `zero_client/index.html` title and brand headers if desired: `zero-network.pages.dev` or retain `zero-chat.pages.dev` / custom domain.
  *(Default assumption: Keep Pages project as `zero-chat` unless user specifies migrating Pages as well).*

### REQ-053.5: Quality Gates & Verification
- Execute `cargo test --workspace` (all 122+ tests pass).
- Execute `cargo clippy --workspace --all-targets -- -D warnings` (clean).
- Execute `cargo fmt -- --check` (clean).
- Execute `./scripts/deploy_dry_run.sh` (clean).
- Execute `bash zero_client/tests/browser/run_test.sh` (all 24 browser tests pass).

---

## 3. Blast Radius & Affected Files

- `zero_client/index.html` (Primary: worker origins)
- `zero_client/tests/browser/test_mesh_scenario.js` (Test URLs)
- `docs/deployment.md` (Documentation)
- `.github/workflows/deploy.yml` (Optional: Pages project name if migrating Pages)

---

## 4. Verification Plan

1. Verify string references:
   `grep -rn "zero-chat.workers.dev" zero_client docs` returns 0 results.
2. Full quality gate execution:
   `cargo test --workspace`
   `cargo clippy --workspace --all-targets -- -D warnings`
   `cargo fmt -- --check`
   `./scripts/deploy_dry_run.sh`
   `bash zero_client/tests/browser/run_test.sh`
