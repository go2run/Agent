#!/usr/bin/env bash
# test.sh — Run all Agent tests (native + WASM)
set -uo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

PASS=0
FAIL=0
SKIP=0

run() {
    local label="$1"; shift
    printf "${CYAN}▶ %s${NC}\n" "$label"
    local output
    output=$("$@" 2>&1) && local rc=0 || local rc=$?
    echo "$output" | tail -8
    if [ "$rc" -eq 0 ]; then
        printf "${GREEN}  ✓ %s${NC}\n\n" "$label"
        PASS=$((PASS + 1))
    else
        printf "${RED}  ✗ %s${NC}\n\n" "$label"
        FAIL=$((FAIL + 1))
    fi
}

skip() {
    local label="$1"
    printf "  ⊘ %s (skipped)\n\n" "$label"
    SKIP=$((SKIP + 1))
}

echo ""
printf "${BOLD}═══════════════════════════════════════════${NC}\n"
printf "${BOLD}  Agent Test Suite${NC}\n"
printf "${BOLD}═══════════════════════════════════════════${NC}\n\n"

# ── 1. Native tests ──────────────────────────────────────
printf "${BOLD}── Native (cargo test) ──${NC}\n\n"

run "agent-types  [native]"    cargo test -p agent-types
run "agent-core   [native]"    cargo test -p agent-core
run "agent-platform [native]"  cargo test -p agent-platform

# ── 2. WASM/Node tests ───────────────────────────────────
printf "${BOLD}── WASM / Node (wasm-pack test --node) ──${NC}\n\n"

if command -v wasm-pack &>/dev/null; then
    run "agent-types  [wasm-node]"    wasm-pack test --node crates/agent-types
    run "agent-core   [wasm-node]"    wasm-pack test --node crates/agent-core
    run "agent-platform [wasm-node]"  wasm-pack test --node crates/agent-platform
else
    skip "WASM/Node tests (wasm-pack not found)"
fi

# ── 3. Summary ────────────────────────────────────────────
echo ""
printf "${BOLD}═══════════════════════════════════════════${NC}\n"
printf "  ${GREEN}Passed: %d${NC}  ${RED}Failed: %d${NC}  Skipped: %d\n" "$PASS" "$FAIL" "$SKIP"
printf "${BOLD}═══════════════════════════════════════════${NC}\n"

[ "$FAIL" -eq 0 ]
