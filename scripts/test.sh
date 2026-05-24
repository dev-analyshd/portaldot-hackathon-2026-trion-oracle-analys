#!/usr/bin/env bash
# test.sh — Run all tests for TRION PortalDot Oracle
set -euo pipefail

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║     TRION PortalDot Oracle — Test Suite                     ║"
echo "╚══════════════════════════════════════════════════════════════╝"

PASS=0
FAIL=0

run_test() {
    local name="$1"
    local cmd="$2"
    local dir="${3:-.}"
    echo ""
    echo "── $name ──"
    if (cd "$dir" && eval "$cmd"); then
        echo "  ✓ PASS"
        PASS=$((PASS+1))
    else
        echo "  ✗ FAIL"
        FAIL=$((FAIL+1))
    fi
}

# Ink! unit tests
run_test "TRIONSignalGate unit tests" \
    "cargo test --features std 2>&1 | tail -20" \
    "contracts/trion_signal_gate"

run_test "BTCPEscrow unit tests" \
    "cargo test --features std 2>&1 | tail -20" \
    "contracts/btcp_escrow"

run_test "BehavioralLimitOrder unit tests" \
    "cargo test --features std 2>&1 | tail -20" \
    "contracts/behavioral_limit_order"

# Oracle bridge tests
run_test "Oracle bridge tests" \
    "cargo test --workspace 2>&1 | tail -20"

# Python syntax check
run_test "Python SDK syntax" \
    "python3 -m py_compile sdk/portaldot_client.py sdk/deploy.py sdk/demo.py && echo OK"

# TRION API check
run_test "TRION Oracle API reachability" \
    "curl -s --max-time 5 '${ORACLE_API_URL:-http://127.0.0.1:5000}/api/v1/whitepaper/coverage' | python3 -c \"import sys,json; d=json.load(sys.stdin); print('formulas:', d.get('total_formulas','?'), '| coverage:', d.get('coverage_pct','?'), '%')\""

echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "  Results: $PASS passed, $FAIL failed"
echo "═══════════════════════════════════════════════════════════════"
[ $FAIL -eq 0 ] && exit 0 || exit 1
