#!/usr/bin/env bash
# local_node.sh — Download and run a PortalDot local development node
#
# Usage:
#   bash scripts/local_node.sh           # auto-detects OS, downloads binary, runs --dev
#   bash scripts/local_node.sh --stop    # kill any running dev node
#   bash scripts/local_node.sh --status  # check if node is running
#
# What this gives you:
#   ws://127.0.0.1:9944     WebSocket RPC
#   http://127.0.0.1:9933   HTTP RPC
#   Alice pre-funded with 1,000,000 POT
#   Block time: ~6s (Aura consensus, --dev mode)
#
# After this starts, run in another terminal:
#   bash scripts/deploy.sh --local
#
set -euo pipefail

NODE_DIR=".local-node"
NODE_BIN="$NODE_DIR/portaldot"
NODE_LOG="$NODE_DIR/node.log"
NODE_PID="$NODE_DIR/node.pid"

# Known PortalDot node binary download page
# https://portaldot-dev.readthedocs.io/en/latest/chain-info.html
DOCS_URL="https://portaldot-dev.readthedocs.io/en/latest/chain-info.html"

# ── Helpers ───────────────────────────────────────────────────────────────────
detect_os() {
    case "$(uname -s)" in
        Linux*)  echo "ubuntu" ;;
        Darwin*) echo "macos" ;;
        CYGWIN*|MINGW*|MSYS*) echo "windows" ;;
        *)       echo "unknown" ;;
    esac
}

node_running() {
    if [[ -f "$NODE_PID" ]]; then
        pid=$(cat "$NODE_PID")
        kill -0 "$pid" 2>/dev/null && return 0
    fi
    pgrep -f "portaldot.*--dev" >/dev/null 2>&1 && return 0
    return 1
}

wait_for_node() {
    echo "  Waiting for node to produce first block..."
    for i in $(seq 1 30); do
        if python3 - 2>/dev/null << 'PYEOF'
from substrateinterface import SubstrateInterface
s = SubstrateInterface(url="ws://127.0.0.1:9944", ss58_format=42)
b = s.get_block_number(None)
print(f"  ✓ Node live: {s.chain} — block #{b}")
s.close()
PYEOF
        then
            return 0
        fi
        echo "    attempt $i/30 — waiting 3s..."
        sleep 3
    done
    echo "  ✗ Node did not start within 90s"
    return 1
}

# ── Commands ──────────────────────────────────────────────────────────────────
if [[ "${1:-}" == "--stop" ]]; then
    if node_running; then
        pkill -f "portaldot.*--dev" 2>/dev/null || true
        [[ -f "$NODE_PID" ]] && kill "$(cat "$NODE_PID")" 2>/dev/null || true
        echo "✓ Node stopped"
    else
        echo "  Node not running"
    fi
    exit 0
fi

if [[ "${1:-}" == "--status" ]]; then
    if node_running; then
        python3 - 2>/dev/null << 'PYEOF' || echo "  Node process running but RPC not yet ready"
from substrateinterface import SubstrateInterface
s = SubstrateInterface(url="ws://127.0.0.1:9944", ss58_format=42)
b = s.get_block_number(None)
r = s.query("System","Account",["5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"])
free = int(r.value["data"]["free"]) if r else 0
print(f"  ✓ Chain: {s.chain}  Block: #{b}  Alice balance: {free/1e14:.2f} POT")
s.close()
PYEOF
    else
        echo "  ✗ Node not running — start with: bash scripts/local_node.sh"
    fi
    exit 0
fi

# ── Main: download + start ────────────────────────────────────────────────────
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║     TRION PortalDot — Local Development Node               ║"
echo "╚══════════════════════════════════════════════════════════════╝"

if node_running; then
    echo "  ✓ Node already running"
    bash "$0" --status
    exit 0
fi

mkdir -p "$NODE_DIR"
OS=$(detect_os)
echo "  OS: $OS"

# ── Download binary if not present ───────────────────────────────────────────
if [[ ! -f "$NODE_BIN" ]]; then
    echo ""
    echo "  Fetching node binary download link from docs..."

    # Scrape download links from PortalDot chain-info docs
    DOWNLOAD_URL=$(python3 - << PYEOF 2>/dev/null
import urllib.request, re
try:
    with urllib.request.urlopen("$DOCS_URL", timeout=10) as r:
        html = r.read().decode()
    # Find download links — look for direct binary links
    links = re.findall(r'href=["\']([^"\']+(?:portaldot|node)[^"\']*)["\']', html, re.I)
    for l in links:
        if '$OS' in l.lower() or 'ubuntu' in l.lower() or 'linux' in l.lower():
            print(l)
            break
    else:
        # Fall back: print all links that look like binaries
        for l in links[:3]:
            print(l)
except Exception as e:
    pass
PYEOF
    )

    if [[ -z "$DOWNLOAD_URL" ]]; then
        echo ""
        echo "  ✗ Could not auto-fetch download link."
        echo ""
        echo "  Manual steps:"
        echo "  1. Open: $DOCS_URL"
        echo "  2. Download the $OS binary for 'Portaldot mainnet node client'"
        echo "     (or 'Portaldot local development node client')"
        echo "  3. Move it to: $NODE_BIN"
        echo "  4. chmod +x $NODE_BIN"
        echo "  5. Re-run: bash scripts/local_node.sh"
        echo ""
        echo "  ─────────────────────────────────────────────────────────"
        echo "  Alternative: use Substrate node template (same --dev mode)"
        echo "  ─────────────────────────────────────────────────────────"
        echo "  If you have a generic substrate-node, it also works:"
        echo "    substrate --dev --ws-port 9944 --rpc-port 9933"
        echo ""
        echo "  Or use Docker (if available):"
        echo "    docker run -p 9944:9944 -p 9933:9933 \\"
        echo "      parity/substrate:latest --dev --ws-external --rpc-external"
        echo ""
        exit 1
    fi

    # Make absolute if relative
    [[ "$DOWNLOAD_URL" != http* ]] && DOWNLOAD_URL="https://portaldot-dev.readthedocs.io$DOWNLOAD_URL"

    echo "  Downloading from: $DOWNLOAD_URL"
    curl -L --progress-bar "$DOWNLOAD_URL" -o "$NODE_BIN"
    chmod +x "$NODE_BIN"
    echo "  ✓ Downloaded: $NODE_BIN"
else
    echo "  ✓ Node binary already present: $NODE_BIN"
fi

# ── Start node in background ──────────────────────────────────────────────────
echo ""
echo "  Starting PortalDot dev node..."
echo "  Flags: --dev --tmp --ws-port 9944 --rpc-port 9933 --ws-external"
echo ""

"$NODE_BIN" \
    --dev \
    --tmp \
    --ws-port 9944 \
    --rpc-port 9933 \
    --ws-external \
    --rpc-external \
    --rpc-cors all \
    > "$NODE_LOG" 2>&1 &

echo $! > "$NODE_PID"
echo "  ✓ Node started (PID $(cat $NODE_PID))"
echo "  Log: $NODE_LOG"
echo "  Tail logs: tail -f $NODE_LOG"
echo ""

# Wait for it to be ready
wait_for_node

echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "  Dev node ready!"
echo ""
echo "  WS RPC  : ws://127.0.0.1:9944"
echo "  HTTP RPC: http://127.0.0.1:9933"
echo ""
echo "  Alice   : 5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"
echo "  Balance : pre-funded (1,000,000+ POT on --dev)"
echo ""
echo "  Now deploy in another terminal:"
echo "    bash scripts/deploy.sh --local"
echo ""
echo "  Stop the node:"
echo "    bash scripts/local_node.sh --stop"
echo "═══════════════════════════════════════════════════════════════"
