#!/usr/bin/env bash
set -euo pipefail

# ==============================================================================
# Local Dry-Run Build Verification for ZeroChat
#
# Validates worker compilation, client WASM compilation, and static bundle
# assembly without performing actual remote deployments.
# ==============================================================================

echo "================================================================="
echo " ZeroChat Deployment Dry-Run Verification"
echo "================================================================="

# Determine project root from script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${ROOT_DIR}"

echo "Project root: ${ROOT_DIR}"

# Step 1: Verify prerequisites
echo "--> Checking prerequisites..."
if ! command -v cargo >/dev/null 2>&1; then
    echo "Error: cargo is required but not installed." >&2
    exit 1
fi

if ! command -v rustup >/dev/null 2>&1; then
    echo "Error: rustup is required but not installed." >&2
    exit 1
fi

echo "--> Ensuring wasm32-unknown-unknown target is installed..."
rustup target add wasm32-unknown-unknown

# Step 2: Validate zero_network compilation
echo "--> Validating zero_network compilation for wasm32-unknown-unknown..."
cargo check -p zero_network --target wasm32-unknown-unknown

if command -v worker-build >/dev/null 2>&1; then
    echo "--> Testing worker-build --release in zero_network..."
    (cd zero_network && worker-build --release)
else
    echo "--> [Notice] worker-build is not installed locally. Skipping worker-build step."
    echo "    (In CI, worker-build is installed and run by wrangler-action)."
fi

# Step 3: Build zero_client release WASM
echo "--> Building zero_client release WASM..."
cargo build -p zero_client --release --target wasm32-unknown-unknown

# Step 4: Assemble static bundle in dist/
echo "--> Assembling static bundle in dist/..."
mkdir -p dist/pkg

CLIENT_WASM="target/wasm32-unknown-unknown/release/zero_client.wasm"

if command -v wasm-bindgen >/dev/null 2>&1; then
    echo "--> Generating web bindings with wasm-bindgen..."
    wasm-bindgen --target web --out-dir dist/pkg "${CLIENT_WASM}"
else
    echo "--> [Notice] wasm-bindgen CLI not found locally. Copying raw wasm as fallback."
    cp "${CLIENT_WASM}" dist/pkg/zero_client_bg.wasm
fi

echo "--> Copying web assets (index.html, style.css, test_harness.html)..."
cp zero_client/index.html dist/
cp zero_client/style.css dist/
cp zero_client/test_harness.html dist/

# Step 5: Assert bundle integrity
echo "--> Verifying assembled bundle integrity..."
if [[ ! -f "dist/index.html" ]]; then
    echo "Error: dist/index.html missing!" >&2
    exit 1
fi

if [[ ! -f "dist/style.css" ]]; then
    echo "Error: dist/style.css missing!" >&2
    exit 1
fi

WASM_COUNT=$(find dist/pkg -type f -name "*.wasm" | wc -l | tr -d ' ')
if [[ "${WASM_COUNT}" -eq 0 ]]; then
    echo "Error: No .wasm files found in dist/pkg/!" >&2
    exit 1
fi

# Step 6: Print artifact details
echo "================================================================="
echo " Assembled Artifacts in dist/:"
ls -lh dist/
echo "--- pkg contents ---"
ls -lh dist/pkg/
echo "================================================================="
echo " Dry-run verification completed successfully!"
echo "================================================================="
