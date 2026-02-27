#!/usr/bin/env bash
# start_all.sh — Build & serve the WASM Agent in the browser
set -euo pipefail

BOLD='\033[1m'
CYAN='\033[0;36m'
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

PORT="${PORT:-8080}"
HOST="${HOST:-127.0.0.1}"

info()  { printf "${CYAN}▶ %s${NC}\n" "$1"; }
ok()    { printf "${GREEN}✓ %s${NC}\n" "$1"; }
fail()  { printf "${RED}✗ %s${NC}\n" "$1"; exit 1; }

# ── Pre-flight checks ────────────────────────────────────
info "Checking dependencies..."
command -v rustup         &>/dev/null || fail "rustup not found. Install: https://rustup.rs"
command -v cargo          &>/dev/null || fail "cargo not found"
command -v wasm-bindgen   &>/dev/null || fail "wasm-bindgen not found. Run: cargo install wasm-bindgen-cli"

if ! rustup target list --installed | grep -q wasm32-unknown-unknown; then
    info "Installing wasm32-unknown-unknown target..."
    rustup target add wasm32-unknown-unknown
fi
ok "All dependencies ready"

# ── Optional: run tests ──────────────────────────────────
if [ "${SKIP_TESTS:-}" != "1" ]; then
    info "Running tests..."
    if bash test.sh; then
        ok "All tests passed"
    else
        fail "Tests failed. Set SKIP_TESTS=1 to skip."
    fi
fi

# ── Build ─────────────────────────────────────────────────
info "Building WASM..."
bash build.sh release
ok "Build complete"

# ── Serve ─────────────────────────────────────────────────
echo ""
printf "${BOLD}═══════════════════════════════════════════${NC}\n"
printf "${BOLD}  WASM Agent${NC}\n"
printf "${BOLD}  http://%s:%s${NC}\n" "$HOST" "$PORT"
printf "${BOLD}═══════════════════════════════════════════${NC}\n\n"

cd dist
if command -v python3 &>/dev/null; then
    python3 -m http.server "$PORT" --bind "$HOST"
elif command -v python &>/dev/null; then
    python -m http.server "$PORT" --bind "$HOST"
elif command -v npx &>/dev/null; then
    npx serve -l "$PORT" -s .
else
    fail "No HTTP server found. Install python3 or node."
fi
