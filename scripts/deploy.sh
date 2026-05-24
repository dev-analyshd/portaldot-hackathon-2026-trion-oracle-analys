#!/usr/bin/env bash
# deploy.sh — Build and deploy all TRION PortalDot Oracle contracts
# Usage: DOT_MNEMONIC="..." bash scripts/deploy.sh
set -euo pipefail

ORACLE_API_URL="${ORACLE_API_URL:-http://127.0.0.1:5000}"
PORTALDOT_RPC="${PORTALDOT_RPC_URL:-wss://rpc.portaldot.io}"

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║     TRION Behavioral Oracle — PortalDot Deployment          ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo "  Oracle API : $ORACLE_API_URL"
echo "  RPC        : $PORTALDOT_RPC"
echo ""

# 0. Prerequisites
echo "[0/6] Checking prerequisites..."
command -v cargo     >/dev/null || { echo "cargo not found — install rustup"; exit 1; }
command -v rustup    >/dev/null || { echo "rustup not found"; exit 1; }

rustup target add wasm32-unknown-unknown 2>/dev/null || true

if ! cargo contract --version >/dev/null 2>&1; then
    echo "  Installing cargo-contract..."
    cargo install cargo-contract --locked
fi
echo "  ✓ cargo-contract $(cargo contract --version)"

# 1. Build TRIONSignalGate
echo ""
echo "[1/6] Building TRIONSignalGate..."
cd contracts/trion_signal_gate
cargo test --features std
cargo contract build --release
echo "  ✓ trion_signal_gate.contract"
cd ../..

# 2. Build BTCPEscrow
echo ""
echo "[2/6] Building BTCPEscrow..."
cd contracts/btcp_escrow
cargo test --features std
cargo contract build --release
echo "  ✓ btcp_escrow.contract"
cd ../..

# 3. Build BehavioralLimitOrder
echo ""
echo "[3/6] Building BehavioralLimitOrder..."
cd contracts/behavioral_limit_order
cargo test --features std
cargo contract build --release
echo "  ✓ behavioral_limit_order.contract"
cd ../..

# 4. Deploy
echo ""
echo "[4/6] Deploying to PortalDot..."
if [ -z "${DOT_MNEMONIC:-}" ]; then
    echo "  DOT_MNEMONIC not set — showing deployment commands:"
    echo ""
    echo "  # Deploy TRIONSignalGate"
    echo "  cargo contract instantiate \\"
    echo "    --contract contracts/trion_signal_gate/target/ink/trion_signal_gate.contract \\"
    echo "    --constructor new \\"
    echo "    --args '0x$(echo -n $ORACLE_API_URL | xxd -p | tr -d '\n')' \\"
    echo "    --suri \"\$DOT_MNEMONIC\" \\"
    echo "    --url $PORTALDOT_RPC \\"
    echo "    --execute"
    echo ""
else
    python3 sdk/deploy.py
fi

# 5. Build oracle bridge
echo ""
echo "[5/6] Building oracle bridge..."
cargo build --release --bin trion-portaldot-bridge
echo "  ✓ oracle bridge binary"

# 6. Run demo
echo ""
echo "[6/6] Running demo signals..."
python3 sdk/demo.py

echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "  Deployment complete."
echo "  To run the oracle bridge: cargo run --release --bin trion-portaldot-bridge"
echo "═══════════════════════════════════════════════════════════════"
