---
ticket_id: "unification-011"
title: "Repair virtual_dog_walker CI and migrate game client to Cloudflare Pages (virtual-dog-walker.pages.dev)"
state: "COMPLETED"
created_at: "2026-11-03T21:20:00Z"
target_repos:
  - "../virtual_dog_walker"
max_review_cycles: 3
---

# Change Proposal: unification-011 — CI Repair & Cloudflare Pages Migration

## 1. Problem Statement
1. **CI regression from unification-010**: `.github/workflows/ci.yml` still runs `pip install -r requirements.txt` (file deleted). The `test` job fails, and the `deploy` job (`needs: test`) never runs. Runs `41719649773` and `41719677561` failed on `main`.
2. **Deploy target is a legacy Worker**: the game is served at `play.zero-network.workers.dev` via `wrangler deploy`. unification-010 also changed `wrangler.jsonc` `name` from `play` to `virtual-dog-walker` and dropped `compatibility_flags`/`ASSETS` binding, so a Wrangler deploy would now create a *different* Worker.
3. **Intended target**: Cloudflare Pages project `virtual-dog-walker` at `https://virtual-dog-walker.pages.dev`, matching the `zero-chat` Pages pattern in `zero_chat/.github/workflows/deploy.yml`.
4. **Decision (user)**: retire `play.zero-network.workers.dev` after the Pages deployment is verified.

## 2. Scope

### 2.1 CI workflow (`virtual_dog_walker/.github/workflows/ci.yml`)
- `test` job: remove `pip install` and pip cache; add Rust toolchain (`wasm32-unknown-unknown`), `wasm-bindgen-cli 0.2.129`; checkout sibling repos `zero_protocol` and `zero_chat` (`${{ github.repository_owner }}/zero-protocol`, `/zero-chat`) beside the workspace so `scripts/build_wasm.sh` resolves `../zero_protocol` and `../zero_chat`; run `cargo test` and `python3 test/run_tests.py`.
- `deploy` job (`needs: test`, push/dispatch on `main`): `npm run build` (→ `dist/`), ensure Pages project exists (`wrangler pages project create virtual-dog-walker --production-branch=main || true`), then `wrangler pages deploy dist --project-name=virtual-dog-walker --branch=main`. Keep the existing skip-with-warning when `CLOUDFLARE_API_TOKEN` is absent.

### 2.2 Build & config
- `scripts/build_pages.sh`: dist must contain only runtime assets (`index.html`, `src/`), exclude `*.py`/`*.md`/tests, and not rely on `.assetsignore`; no `wrangler.jsonc`/`package.json` required in `dist/`.
- `wrangler.jsonc`: convert to Pages config (`name: "virtual-dog-walker"`, `pages_build_output_dir: "./dist"`); remove Workers-assets `assets` block. `package.json` scripts: `deploy` → `npm run build && wrangler pages deploy dist --project-name=virtual-dog-walker`; `dev` → `wrangler pages dev dist` after build.
- `src/net/network.js`: verify `*.pages.dev` hosts select `ZeroMeshNetworkManager` with `clifton-commons` (already true via `pages.dev` check); drop remaining dead PartyKit/dedicated-server branches only if no test depends on them.
- `README.md`: document the Pages URL and CI/deploy flow.

### 2.3 Retirement of `play.zero-network.workers.dev` (post-verification, manual/operator step)
- After `https://virtual-dog-walker.pages.dev` serves the game and two clients connect via `network.zero-network.workers.dev`, delete the `play` Worker (`wrangler delete play` or dashboard). This step requires the user's Cloudflare credentials and is **not** automated by workers; the spec records it as an operator checklist item.
- Update any references to `play.zero-network.workers.dev` in docs/CORS allow-lists (grep `zero_network` and `zero_chat` for the hostname) to the new Pages origin.

## 3. Acceptance Criteria
- **AC-011.1**: CI `test` job passes on GitHub for `virtual_dog_walker` `main` (no `requirements.txt` dependency).
- **AC-011.2**: `npm run build` produces `dist/` containing `index.html`, `src/` (incl. both Wasm bundles and `zero_chat_widget.*`) and no `*.py`, `*.md`, `test/`, `openspec/`, `.factory/`.
- **AC-011.3**: CI `deploy` job publishes to Pages project `virtual-dog-walker` (`https://virtual-dog-walker.pages.dev` returns 200 and loads the game).
- **AC-011.4**: Two browser clients on the Pages URL see each other and NPCs (seed-host + peer path) via `network.zero-network.workers.dev`.
- **AC-011.5**: `grep -rn "play.zero-network.workers.dev"` across the four repos returns no live references (historical archives excluded).
- **AC-011.6**: `play` Worker deleted by operator after AC-011.3/4 are confirmed.
- **AC-011.7**: `cargo test` and `python3 test/run_tests.py` remain 100% green locally and in CI.

## 4. Risks
- `zero-protocol`/`zero-chat` repos must be readable by the default `GITHUB_TOKEN` (same-owner private repos may need a PAT secret; `zero_chat`'s workflow already relies on this for `zero-protocol`).
- Pages project creation and deployment need `CLOUDFLARE_API_TOKEN` with Pages:Edit and `CLOUDFLARE_ACCOUNT_ID` set in the `virtual-dog-walker` GitHub repo.
- `zero_network` CORS/origin checks may need the new `pages.dev` origin (verify; universal CORS is currently enabled).

## 5. Proposed Brief Decomposition (for implement-task)
1. `brief-ci-workflow` [M]: `ci.yml` (test + Pages deploy).
2. `brief-pages-build-config` [M]: `build_pages.sh`, `wrangler.jsonc`, `package.json`, README.
3. `brief-origin-references` [L]: grep/replace `play.zero-network.workers.dev` across repos; verify `network.js` host detection.
4. Validation + review; operator checklist for Worker retirement.

