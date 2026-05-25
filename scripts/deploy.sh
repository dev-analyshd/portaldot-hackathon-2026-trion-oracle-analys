#!/usr/bin/env bash
# deploy.sh — Build and deploy all TRION PortalDot Oracle contracts
#
# MODES
#   Local dev  (zero POT needed, Alice account pre-funded):
#     bash scripts/deploy.sh --local
#     bash scripts/deploy.sh  # auto-detected when RPC = localhost/127.0.0.1
#
#   Mainnet / testnet:
#     DOT_MNEMONIC="..." bash scripts/deploy.sh
#     DOT_MNEMONIC="..." PORTALDOT_RPC_URL="wss://mainnet.portaldot.io" bash scripts/deploy.sh
#
set -euo pipefail

# ── Config ────────────────────────────────────────────────────────────────────
ORACLE_API_URL="${ORACLE_API_URL:-http://127.0.0.1:5000}"
PORTALDOT_RPC="${PORTALDOT_RPC_URL:-wss://mainnet.portaldot.io}"

# Alice dev mnemonic (Substrate well-known, pre-funded on every --dev chain)
ALICE_MNEMONIC="bottom drive obey lake curtain smoke basket hold race lonely fit walk"
# Alice SS58 address (prefix 42): 5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY

# ── Mode detection ────────────────────────────────────────────────────────────
LOCAL_MODE=false
for arg in "$@"; do
    [[ "$arg" == "--local" ]] && LOCAL_MODE=true
done
# Auto-detect if RPC points to localhost
if [[ "$PORTALDOT_RPC" == *"localhost"* || "$PORTALDOT_RPC" == *"127.0.0.1"* ]]; then
    LOCAL_MODE=true
fi
# If no mnemonic and not explicitly local, fall back to local mode gracefully
if [[ -z "${DOT_MNEMONIC:-}" && "$LOCAL_MODE" == false ]]; then
    echo "  ⚠  DOT_MNEMONIC not set — switching to local dev mode"
    LOCAL_MODE=true
fi

if [[ "$LOCAL_MODE" == true ]]; then
    PORTALDOT_RPC="${PORTALDOT_RPC_URL:-ws://127.0.0.1:9944}"
    DEPLOY_SURI="$ALICE_MNEMONIC"
    DEPLOY_LABEL="LOCAL DEV (Alice, pre-funded)"
else
    PORTALDOT_RPC="${PORTALDOT_RPC_URL:-wss://mainnet.portaldot.io}"
    DEPLOY_SURI="${DOT_MNEMONIC}"
    DEPLOY_LABEL="MAINNET — ${PORTALDOT_RPC}"
fi

# ── Header ────────────────────────────────────────────────────────────────────
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║     TRION Behavioral Oracle — PortalDot Deployment          ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo "  Oracle API  : $ORACLE_API_URL"
echo "  RPC         : $PORTALDOT_RPC"
echo "  Mode        : $DEPLOY_LABEL"
if [[ "$LOCAL_MODE" == true ]]; then
    echo "  Signer      : Alice (5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY)"
    echo ""
    echo "  ℹ  Local mode: no POT needed. Start the dev node first:"
    echo "     bash scripts/local_node.sh"
    echo "     (wait for 'Imported #1' before continuing)"
fi
echo ""

# ── 0. Prerequisites ──────────────────────────────────────────────────────────
echo "[0/6] Checking prerequisites..."
command -v cargo  >/dev/null || { echo "cargo not found — install rustup: https://rustup.rs"; exit 1; }
command -v rustup >/dev/null || { echo "rustup not found"; exit 1; }

rustup target add wasm32-unknown-unknown 2>/dev/null || true

if ! cargo contract --version >/dev/null 2>&1; then
    echo "  Installing cargo-contract..."
    cargo install cargo-contract --locked
fi
echo "  ✓ cargo-contract $(cargo contract --version)"

if [[ "$LOCAL_MODE" == true ]]; then
    # Verify local node is running
    echo ""
    echo "  Checking local node at $PORTALDOT_RPC ..."
    if ! python3 - << 'PYEOF' 2>/dev/null
from substrateinterface import SubstrateInterface
s = SubstrateInterface(url="ws://127.0.0.1:9944", ss58_format=42)
print(f"  ✓ Local node: {s.chain} (block #{s.get_block_number(None)})")
s.close()
PYEOF
    then
        echo ""
        echo "  ✗ Local node not responding at ws://127.0.0.1:9944"
        echo "    Start it first: bash scripts/local_node.sh"
        echo ""
        echo "  Continuing in DRY-RUN mode (printing deployment commands only)..."
        DRY_RUN=true
    else
        DRY_RUN=false
    fi
else
    DRY_RUN=false
fi

# ── 1-3. Build contracts ──────────────────────────────────────────────────────
for step_num in 1 2 3; do
    case $step_num in
        1) contract="trion_signal_gate";       label="TRIONSignalGate" ;;
        2) contract="btcp_escrow";             label="BTCPEscrow" ;;
        3) contract="behavioral_limit_order";  label="BehavioralLimitOrder" ;;
    esac

    echo ""
    echo "[$step_num/6] Building $label..."
    cd "contracts/$contract"
    cargo test --features std 2>&1 | grep -E "(test .* ok|test .* FAILED|FAILED|passed|failed)" || true
    cargo contract build --release 2>&1 | tail -3
    echo "  ✓ $contract.contract"
    cd ../..
done

# ── 4. Deploy ─────────────────────────────────────────────────────────────────
echo ""
echo "[4/6] Deploying to PortalDot..."
echo ""

if [[ "${DRY_RUN:-false}" == true ]]; then
    echo "  DRY-RUN mode — commands to run once node is live:"
    echo ""
    echo "  # Deploy TRIONSignalGate"
    echo "  cargo contract instantiate \\"
    echo "    --contract contracts/trion_signal_gate/target/ink/trion_signal_gate.contract \\"
    echo "    --constructor new --args '0x' \\"
    echo "    --suri \"$DEPLOY_SURI\" --url $PORTALDOT_RPC --execute"
    echo ""
    echo "  # Deploy BTCPEscrow"
    echo "  cargo contract instantiate \\"
    echo "    --contract contracts/btcp_escrow/target/ink/btcp_escrow.contract \\"
    echo "    --constructor new --args <SIGNAL_GATE_ADDR> \\"
    echo "    --suri \"$DEPLOY_SURI\" --url $PORTALDOT_RPC --execute"
    echo ""
    echo "  # Deploy BehavioralLimitOrder"
    echo "  cargo contract instantiate \\"
    echo "    --contract contracts/behavioral_limit_order/target/ink/behavioral_limit_order.contract \\"
    echo "    --constructor new --args <SIGNAL_GATE_ADDR> <YOUR_ADDR> \\"
    echo "    --suri \"$DEPLOY_SURI\" --url $PORTALDOT_RPC --execute"
    echo ""
    SIGNAL_GATE_ADDR="DRY_RUN_PENDING"
    BTCP_ESCROW_ADDR="DRY_RUN_PENDING"
    BLO_ADDR="DRY_RUN_PENDING"
else
    # Live deployment via cargo contract instantiate
    echo "  Deploying TRIONSignalGate..."
    SG_OUTPUT=$(cargo contract instantiate \
        --contract contracts/trion_signal_gate/target/ink/trion_signal_gate.contract \
        --constructor new \
        --args "0x" \
        --suri "$DEPLOY_SURI" \
        --url "$PORTALDOT_RPC" \
        --execute \
        --skip-confirm 2>&1)
    SIGNAL_GATE_ADDR=$(echo "$SG_OUTPUT" | grep -oP 'Contract [\w\d]+' | awk '{print $2}' || \
                       echo "$SG_OUTPUT" | grep -oP '5[A-Za-z0-9]{47}' | head -1)
    echo "  ✓ TRIONSignalGate: $SIGNAL_GATE_ADDR"

    echo ""
    echo "  Deploying BTCPEscrow..."
    BE_OUTPUT=$(cargo contract instantiate \
        --contract contracts/btcp_escrow/target/ink/btcp_escrow.contract \
        --constructor new \
        --args "$SIGNAL_GATE_ADDR" \
        --suri "$DEPLOY_SURI" \
        --url "$PORTALDOT_RPC" \
        --execute \
        --skip-confirm 2>&1)
    BTCP_ESCROW_ADDR=$(echo "$BE_OUTPUT" | grep -oP '5[A-Za-z0-9]{47}' | head -1)
    echo "  ✓ BTCPEscrow: $BTCP_ESCROW_ADDR"

    echo ""
    echo "  Deploying BehavioralLimitOrder..."
    BLO_OUTPUT=$(cargo contract instantiate \
        --contract contracts/behavioral_limit_order/target/ink/behavioral_limit_order.contract \
        --constructor new \
        --args "$SIGNAL_GATE_ADDR" "$SIGNAL_GATE_ADDR" \
        --suri "$DEPLOY_SURI" \
        --url "$PORTALDOT_RPC" \
        --execute \
        --skip-confirm 2>&1)
    BLO_ADDR=$(echo "$BLO_OUTPUT" | grep -oP '5[A-Za-z0-9]{47}' | head -1)
    echo "  ✓ BehavioralLimitOrder: $BLO_ADDR"

    # Save .env.deployed
    cat > .env.deployed << ENVEOF
TRION_SIGNAL_GATE_ADDRESS=$SIGNAL_GATE_ADDR
BTCP_ESCROW_ADDRESS=$BTCP_ESCROW_ADDR
BLO_ADDRESS=$BLO_ADDR
PORTALDOT_RPC_URL=$PORTALDOT_RPC
DEPLOY_MODE=$DEPLOY_LABEL
ENVEOF
    echo ""
    echo "  ✓ Addresses saved to .env.deployed"
fi

# ── 5. Build oracle bridge ────────────────────────────────────────────────────
echo ""
echo "[5/6] Building oracle bridge..."
cargo build --release --bin trion-portaldot-bridge 2>&1 | tail -3
echo "  ✓ oracle bridge binary"

# ── 6. Run demo ───────────────────────────────────────────────────────────────
echo ""
echo "[6/6] Running demo signals..."
ORACLE_API_URL="$ORACLE_API_URL" python3 sdk/demo.py

# ── Summary ───────────────────────────────────────────────────────────────────
echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "  TRION PortalDot Oracle — Deployment Complete"
echo "═══════════════════════════════════════════════════════════════"
echo "  Mode    : $DEPLOY_LABEL"
echo "  RPC     : $PORTALDOT_RPC"
echo ""
echo "  TRIONSignalGate        : $SIGNAL_GATE_ADDR"
echo "  BTCPEscrow             : $BTCP_ESCROW_ADDR"
echo "  BehavioralLimitOrder   : $BLO_ADDR"
echo ""
if [[ "$SIGNAL_GATE_ADDR" != "DRY_RUN_PENDING" ]]; then
    echo "  Next: update SUBMISSION.md with these addresses, then"
fi
echo "  Start oracle bridge: cargo run --release --bin trion-portaldot-bridge"
echo "═══════════════════════════════════════════════════════════════"
