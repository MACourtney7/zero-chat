# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-007"
title: "CI/CD Automation: GitHub Actions & Cloudflare Deployment Pipeline"
state: "PLANNING"
roadmap: "serverless-p2p-chat"
depends_on: ["ticket-003", "ticket-005"]
created_at: "2026-10-16T21:35:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-007/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/serverless-p2p-chat.md`

## 1. Problem Statement & Objective
To ensure cross-target compilation integrity and automated production delivery, we must establish a CI/CD automation pipeline using GitHub Actions. CI verifies code formatting, strict clippy linter passes, multi-crate workspace test execution, and target-specific builds (`wasm32-unknown-unknown` for `zero_client`). CD automates zero-downtime deployment: publishing the `zero_network` Cloudflare Worker & Durable Object via Wrangler, and deploying the compiled `zero_client` static WASM bundle to Cloudflare Pages.

## 2. Scope Boundaries
- **In-Scope**:
  - `.github/workflows/ci.yml`:
    - Rust toolchain setup with `wasm32-unknown-unknown` target.
    - Code quality checks: `cargo fmt --check` and `cargo clippy --workspace -- -D warnings`.
    - Native test suite execution: `cargo test --workspace`.
    - Client WASM build validation: `cargo build -p zero_client --target wasm32-unknown-unknown`.
  - `.github/workflows/deploy.yml`:
    - Automated deployment triggered on push to `main` branch or version tags.
    - Cloudflare Worker deployment: executes `wrangler deploy` in `zero_network/` utilizing `cloudflare/wrangler-action`.
    - Cloudflare Pages deployment: compiles `zero_client` via `wasm-bindgen` and deploys static bundle (`index.html`, `style.css`, generated `.wasm` and `.js`) using `wrangler pages deploy`.
  - Secrets documentation: `docs/deployment.md` outlining setup of `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`.
  - Local validation script: `scripts/deploy_dry_run.sh` allowing developers to verify builds locally before pushing.
- **Out-of-Scope**:
  - End-to-end browser automation in headless browsers inside CI (covered by unit and mock integration tests).
  - Third-party cloud providers beyond GitHub and Cloudflare.

## 3. Requirements & Acceptance Criteria
- [ ] **REQ-007.1**: GitHub Actions CI workflow triggers on pull requests and pushes to `main`.
  - *Verification*: Workflow YAML is syntactically valid and linted with actionlint or schema checker.
- [ ] **REQ-007.2**: CI enforces zero compiler warnings, zero clippy warnings, and zero test failures across all crates.
  - *Verification*: Pipeline passes for clean commits and fails if any test or lint rule fails.
- [ ] **REQ-007.3**: Deployment workflow builds and publishes `zero_network` to Cloudflare using configured secrets.
  - *Verification*: `deploy.yml` correctly targets the Durable Object bindings declared in `zero_network/wrangler.toml`.
- [ ] **REQ-007.4**: Deployment workflow builds and publishes the `zero_client` WASM bundle to Cloudflare Pages.
  - *Verification*: Build artifact directory contains `index.html`, `style.css`, and compiled `.wasm`/`.js` artifacts ready for static hosting.
- [ ] **REQ-007.5**: Deployment instructions in `docs/deployment.md` document all required GitHub repository secrets and Cloudflare dashboard configurations.
  - *Verification*: Documentation contains clear step-by-step setup instructions.

## 4. Architectural Design & Impacted Modules
- **New Modules / Files**:
  - `.github/workflows/ci.yml`
  - `.github/workflows/deploy.yml`
  - `docs/deployment.md`
  - `scripts/deploy_dry_run.sh`
- **Modified Modules**:
  - `zero_network/wrangler.toml` (ensure production environment configurations match GitHub Actions action inputs)

## 5. Decomposition Plan (Architect Briefs)

| Batch | Brief ID | Role | Target Thinking Level | Description | Files Scope | Dependencies |
| :---: | :--- | :--- | :---: | :--- | :--- | :--- |
| **1** | `brief-01-ci-workflow` | Implementer | `LOW` | GitHub Actions CI workflow for fmt, clippy, test, and WASM compilation | `.github/workflows/ci.yml` | None (Parallel) |
| **1** | `brief-02-deploy-workflow` | Implementer | `LOW` | GitHub Actions deploy workflow for Worker and Pages + dry-run script | `.github/workflows/deploy.yml`, `scripts/deploy_dry_run.sh` | None (Parallel) |
| **1** | `brief-03-deploy-docs` | Implementer | `LOW` | Deployment setup and secrets documentation | `docs/deployment.md` | None (Parallel) |
| **2** | `brief-val-workflow-lint` | Validator | `LOW` | Validate workflow syntax, shell scripts, and workspace compatibility | Worktree repo | Batch 1 |
| **3** | `brief-rev` | Reviewer | `HIGH` | Security audit (secret handling, least-privilege tokens, build reproducibility) | Candidate diff | Batch 2 |
