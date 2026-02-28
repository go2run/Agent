#!/usr/bin/env bash
# build.sh — Build the WASM Agent without trunk
set -euo pipefail

BOLD='\033[1m'
CYAN='\033[0;36m'
GREEN='\033[0;32m'
NC='\033[0m'

DIST="dist"
PROFILE="${1:-dev}"

info() { printf "${CYAN}▶ %s${NC}\n" "$1"; }
ok()   { printf "${GREEN}✓ %s${NC}\n" "$1"; }

info "Building agent-app (wasm32-unknown-unknown, profile: $PROFILE)..."

if [ "$PROFILE" = "release" ]; then
    cargo build -p agent-app --target wasm32-unknown-unknown --release
    WASM_DIR="target/wasm32-unknown-unknown/release"
else
    cargo build -p agent-app --target wasm32-unknown-unknown
    WASM_DIR="target/wasm32-unknown-unknown/debug"
fi

info "Running wasm-bindgen..."
wasm-bindgen "$WASM_DIR/agent_app.wasm" \
    --out-dir "$DIST" \
    --target web \
    --no-typescript

if [ "$PROFILE" = "release" ] && command -v wasm-opt &>/dev/null; then
    info "Optimizing WASM with wasm-opt..."
    wasm-opt -Os "$DIST/agent_app_bg.wasm" -o "$DIST/agent_app_bg.wasm"
fi

info "Copying web assets..."
cp web/index.html "$DIST/index.html"
cp web/worker.js "$DIST/worker.js"
cp web/serve.py "$DIST/serve.py"
cp -r web/fonts/* "$DIST/" 2>/dev/null || true

# Download @wasmer/sdk for local serving (avoids COEP/CORS issues)
WASMER_SDK_VER="0.10.0"
SDK_DIR="$DIST/vendor/wasmer-sdk"
if [ ! -f "$SDK_DIR/wasmer_js_bg.wasm" ]; then
    info "Downloading @wasmer/sdk v${WASMER_SDK_VER}..."
    mkdir -p "$SDK_DIR"
    SDK_CDN="https://cdn.jsdelivr.net/npm/@wasmer/sdk@${WASMER_SDK_VER}/dist"
    curl -sL "$SDK_CDN/index.mjs"          -o "$SDK_DIR/index.mjs"
    curl -sL "$SDK_CDN/wasmer_js_bg.wasm"  -o "$SDK_DIR/wasmer_js_bg.wasm"
    curl -sL "$SDK_CDN/worker.mjs"         -o "$SDK_DIR/worker.mjs"
    ok "@wasmer/sdk v${WASMER_SDK_VER} downloaded"
else
    ok "@wasmer/sdk already cached in dist/"
fi

ok "Build complete → $DIST/"
