# Zero-Chat Deployment & Secrets Guide

This document outlines the deployment architecture, required credentials/secrets, Cloudflare dashboard configurations, CI/CD pipelines, and local dry-run validation instructions for Zero-Chat (**REQ-007.5**).

---

## 1. Overview & Architecture

Zero-Chat utilizes a modern, serverless, two-tier architecture hosted on Cloudflare's global edge network:

1. **Edge Signaling & Dead-Drop (`zero_network`)**:
   - **Cloudflare Worker** paired with a hibernating **Durable Object** (`DeadDropRoom`).
   - Handles ephemeral WebRTC signaling, peer discovery, and offline mailbox dead-drops backed by SQLite durable state.
   - Low latency, global coordination point with zero idle compute costs.

2. **Static P2P Web Client (`zero_client`)**:
   - Hosted via **Cloudflare Pages**.
   - Serves the static client application: HTML shell, styling, JavaScript shims, and compiled WebAssembly (`wasm32-unknown-unknown`) package.
   - Operates fully peer-to-peer (P2P) in client browsers once signaled via `zero_network`.

---

## 2. GitHub Repository Secrets

Automated deployment workflows running in GitHub Actions require Cloudflare authentication credentials configured as repository secrets.

### Required Secrets

| Secret Name | Description | Scope / Context |
|---|---|---|
| `CLOUDFLARE_API_TOKEN` | Cloudflare API Bearer Token with Worker & Pages management permissions | GitHub Actions (`deploy.yml`) |
| `CLOUDFLARE_ACCOUNT_ID` | Cloudflare Account Identifier (Hexadecimal string) | GitHub Actions (`deploy.yml`) |

### Generating the Cloudflare API Token

1. Log in to the [Cloudflare Dashboard](https://dash.cloudflare.com/).
2. In the top-right corner, select your profile icon and click **My Profile**.
3. In the left navigation menu, click **API Tokens**.
4. Click **Create Token**, then navigate to **Create Custom Token** and click **Get started**.
5. Set a descriptive name (e.g., `zero-chat-ci-deploy`).
6. Under **Permissions**, add the following minimum permissions:
   - **Account** -> **Workers Scripts** -> **Edit**
   - **Account** -> **Workers KV Storage** -> **Edit**
   - **Account** -> **Cloudflare Pages** -> **Edit**
   - **Account** -> **Account Settings** -> **Read**
7. Under **Account Resources**, choose **Include** -> **All accounts** (or restrict to your target account).
8. Click **Continue to summary**, review, and click **Create Token**.
9. **Copy the token immediately** (it will not be displayed again).

### Locating the Cloudflare Account ID

1. Navigate to the Cloudflare dashboard homepage or **Workers & Pages** -> **Overview**.
2. Look at the right-hand sidebar under **Account Details**.
3. Copy the **Account ID** value.

### Configuring Secrets in GitHub

1. Open your repository on GitHub.
2. Navigate to **Settings** -> **Secrets and variables** -> **Actions**.
3. Under **Repository secrets**, click **New repository secret**:
   - Name: `CLOUDFLARE_API_TOKEN`
   - Secret: Paste the API token generated above.
   - Click **Add secret**.
4. Click **New repository secret** again:
   - Name: `CLOUDFLARE_ACCOUNT_ID`
   - Secret: Paste your Cloudflare Account ID.
   - Click **Add secret**.

---

## 3. Cloudflare Resource Configuration

### Worker & Durable Object (`zero_network`)

The backend signaling worker is defined in `crates/zero_network/wrangler.toml` and requires the following Cloudflare configuration:

- **Worker Name**: `network` (routes to `https://network.zero-chat.workers.dev`)
- **Main Entrypoint**: `build/worker/shim.mjs`
- **Compatibility Date**: `2025-01-01`
- **Compatibility Flags**: `["nodejs_compat"]`
- **Durable Object Binding**:
  - Binding Name: `DEAD_DROP_ROOM`
  - Class Name: `DeadDropRoom`
- **Durable Object Migrations**:
  - Tag: `v1`
  - Classes: `["DeadDropRoom"]` (SQLite storage enabled for persistent dead-drop state)

### Cloudflare Pages (`zero_client`)

The static web application is deployed to Cloudflare Pages:

- **Project Name**: `zero-chat`
- **Production Branch**: `main`
- **Output Directory**: `dist`
- **Build Command** (for direct Cloudflare dashboard builds, if used):
  ```bash
  ./scripts/build_wasm.sh
  ```
  *(Note: Standard deployment uses GitHub Actions via Wrangler CLI to push pre-compiled bundles directly to Cloudflare Pages).*

---

## 4. CI/CD Automation Pipelines

Automated checks and deployment procedures are orchestrated via GitHub Actions workflows:

### Continuous Integration (`.github/workflows/ci.yml`)
Runs automatically on all pull requests and pushes to `main`:
- **Code Formatting**: Checks Rust code formatting across all crates via `cargo fmt --all -- --check`.
- **Clippy Linting**: Enforces strict code quality without warnings using `cargo clippy --workspace --all-targets -- -D warnings`.
- **Test Suite**: Runs the full suite of unit and integration tests across the workspace via `cargo test --workspace`.
- **WASM Verification**: Validates compilation targeting `wasm32-unknown-unknown` for `zero_client`.

### Continuous Deployment (`.github/workflows/deploy.yml`)
Triggers automatically on pushes to `main` and version tags (`v*`):
- Sets up Rust toolchain with target `wasm32-unknown-unknown`.
- Installs `wasm-bindgen-cli` and Node.js dependencies (`wrangler`).
- Builds the WASM client bundle into `dist/`.
- Deploys the static client to **Cloudflare Pages** project `zero-chat`.
- Deploys the signaling Worker and Durable Object to **Cloudflare Workers** (`zero_network`).

---

## 5. Local Pre-flight Dry-Run

Before pushing changes to GitHub or deploying to production, validate the build and packaging pipeline locally using the dry-run script.

### Prerequisites

Ensure the following tools are installed on your workstation:

```bash
# 1. Rust and wasm32 compilation target
rustup target add wasm32-unknown-unknown

# 2. wasm-bindgen CLI matching your wasm-bindgen crate version
cargo install wasm-bindgen-cli

# 3. Cloudflare Wrangler CLI (via npm)
npm install -g wrangler
```

### Running the Local Dry-Run Script

Run the pre-flight verification script from the root of the repository:

```bash
./scripts/deploy_dry_run.sh
```

### Verifying Build Artifacts

Upon successful execution, the script will:
1. Verify required prerequisites and tool versions.
2. Compile `crates/zero_client` to `wasm32-unknown-unknown`.
3. Run `wasm-bindgen` and package static assets into `dist/`.
4. Validate that `dist/index.html`, `dist/zero_client.js`, and `dist/zero_client_bg.wasm` exist and are non-empty.
5. Perform a dry-run check of `crates/zero_network/wrangler.toml` configuration using `wrangler deploy --dry-run`.
