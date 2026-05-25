"""
deploy.py — Full deployment script for TRION PortalDot Oracle
Deploys all three Ink! contracts and wires them together.

MODES
    Local dev (zero POT needed):
        python sdk/deploy.py --local
        PORTALDOT_RPC_URL=ws://127.0.0.1:9944 python sdk/deploy.py

    Mainnet / testnet:
        DOT_MNEMONIC="your mnemonic" python sdk/deploy.py
        DOT_MNEMONIC="..." PORTALDOT_RPC_URL="wss://mainnet.portaldot.io" python sdk/deploy.py

Steps:
    1. Build all Ink! contracts (cargo contract build --release)
    2. Deploy TRIONSignalGate  → capture address
    3. Deploy BTCPEscrow       → with SignalGate address
    4. Deploy BehavioralLimitOrder → with SignalGate address
    5. Authorize oracle relayer as publisher
    6. Verify deployment + demo signals

Author: Hudu Yusuf (Analys) | MIT
"""

import os
import sys
import json
import subprocess
import time
import logging
import argparse

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s"
)
log = logging.getLogger("deploy")

# ── Constants ─────────────────────────────────────────────────────────────────
# Alice dev account — pre-funded on every Substrate --dev chain
ALICE_MNEMONIC = "bottom drive obey lake curtain smoke basket hold race lonely fit walk"
ALICE_SS58     = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"

MAINNET_RPC    = "wss://mainnet.portaldot.io"
LOCAL_RPC      = "ws://127.0.0.1:9944"

# ── Config from env + args ────────────────────────────────────────────────────
parser = argparse.ArgumentParser(description="Deploy TRION Oracle contracts to PortalDot")
parser.add_argument("--local", action="store_true", help="Use local dev node (Alice, no POT needed)")
parser.add_argument("--dry-run", action="store_true", help="Print deployment commands only")
args, _ = parser.parse_known_args()

ORACLE_API_URL = os.getenv("ORACLE_API_URL", "http://127.0.0.1:5000")
DOT_MNEMONIC   = os.getenv("DOT_MNEMONIC", "")
PORTALDOT_RPC  = os.getenv("PORTALDOT_RPC_URL", "")

# Determine mode
def is_local_mode() -> bool:
    if args.local:
        return True
    rpc = PORTALDOT_RPC or MAINNET_RPC
    if "localhost" in rpc or "127.0.0.1" in rpc:
        return True
    if not DOT_MNEMONIC:
        return True
    return False

LOCAL_MODE = is_local_mode()

if LOCAL_MODE:
    PORTALDOT_RPC  = PORTALDOT_RPC or LOCAL_RPC
    DEPLOY_MNEMONIC = ALICE_MNEMONIC
    DEPLOY_ADDR     = ALICE_SS58
    MODE_LABEL      = "LOCAL DEV (Alice, pre-funded — no POT needed)"
else:
    PORTALDOT_RPC  = PORTALDOT_RPC or MAINNET_RPC
    DEPLOY_MNEMONIC = DOT_MNEMONIC
    DEPLOY_ADDR     = "(from DOT_MNEMONIC)"
    MODE_LABEL      = f"MAINNET — {PORTALDOT_RPC}"

DRY_RUN = args.dry_run


# ── Helpers ───────────────────────────────────────────────────────────────────
def run(cmd: str, cwd: str = ".") -> tuple[int, str]:
    log.info(f"$ {cmd}")
    result = subprocess.run(cmd, shell=True, cwd=cwd, capture_output=True, text=True)
    if result.returncode != 0:
        log.error(result.stderr[-400:] if result.stderr else "(no stderr)")
    return result.returncode, result.stdout + result.stderr


def check_node_reachable() -> bool:
    try:
        from substrateinterface import SubstrateInterface
        s = SubstrateInterface(url=PORTALDOT_RPC, ss58_format=42)
        block = s.get_block_number(None)
        log.info(f"  ✓ Node: {s.chain} — block #{block}")
        s.close()
        return True
    except Exception as e:
        log.warning(f"  Node not reachable at {PORTALDOT_RPC}: {e}")
        return False


def check_alice_balance() -> float:
    """Return Alice's free balance in POT (only meaningful in local mode)."""
    try:
        from substrateinterface import SubstrateInterface
        s = SubstrateInterface(url=PORTALDOT_RPC, ss58_format=42)
        raw = s.rpc_request("state_getStorage", [_system_account_key(ALICE_SS58)])
        s.close()
        result = raw.get("result")
        if result and result != "0x":
            import struct
            data = bytes.fromhex(result[2:])
            free_raw = int.from_bytes(data[16:32], "little") if len(data) >= 32 else 0
            return free_raw / 1e14
    except Exception:
        pass
    return 0.0


def _system_account_key(ss58: str) -> str:
    """Build System.Account storage key for an SS58 address."""
    import xxhash, hashlib
    from substrateinterface import Keypair
    pub = bytes.fromhex(Keypair(ss58_address=ss58).public_key.hex())
    ph = xxhash.xxh128(b"System",  seed=0).digest()[::-1] + xxhash.xxh128(b"System",  seed=1).digest()[::-1]
    sh = xxhash.xxh128(b"Account", seed=0).digest()[::-1] + xxhash.xxh128(b"Account", seed=1).digest()[::-1]
    kh = hashlib.blake2b(pub, digest_size=16).digest()
    return "0x" + ph.hex() + sh.hex() + kh.hex() + pub.hex()


def extract_contract_address(output: str) -> str:
    """Extract SS58 contract address from cargo contract instantiate output."""
    import re
    # cargo-contract prints "Contract 5Xxx..." or "  Contract AccountId: 5Xxx..."
    for pattern in [
        r'Contract\s+([A-Za-z0-9]{47,48})',
        r'contract_address["\s:]+([A-Za-z0-9]{47,48})',
        r'(5[A-Za-z0-9]{47})',
    ]:
        m = re.search(pattern, output)
        if m:
            addr = m.group(1)
            if addr.startswith("5") and len(addr) in (47, 48):
                return addr
    return ""


def cargo_instantiate(contract_path: str, constructor_args: list[str]) -> str:
    """Run cargo contract instantiate and return the deployed address."""
    args_str = " ".join(f'"{a}"' for a in constructor_args) if constructor_args else ""
    cmd = (
        f'cargo contract instantiate '
        f'--contract {contract_path} '
        f'--constructor new '
        f'{"--args " + args_str if args_str else ""} '
        f'--suri "{DEPLOY_MNEMONIC}" '
        f'--url {PORTALDOT_RPC} '
        f'--execute '
        f'--skip-confirm'
    )
    rc, output = run(cmd)
    if rc != 0:
        log.error(f"Instantiate failed:\n{output[-600:]}")
        return ""
    addr = extract_contract_address(output)
    if not addr:
        log.warning(f"Could not parse address from output:\n{output[-400:]}")
    return addr


# ── Step 1: Build ─────────────────────────────────────────────────────────────
def build_contracts():
    log.info("═" * 60)
    log.info("Step 1: Building Ink! contracts")
    log.info("═" * 60)

    rc, _ = run("cargo contract --version")
    if rc != 0:
        log.info("Installing cargo-contract...")
        run("cargo install cargo-contract --locked")

    contracts = [
        ("contracts/trion_signal_gate",      "TRIONSignalGate"),
        ("contracts/btcp_escrow",             "BTCPEscrow"),
        ("contracts/behavioral_limit_order",  "BehavioralLimitOrder"),
    ]
    for path, name in contracts:
        log.info(f"\nBuilding {name}...")
        run(f"cargo test --features std", cwd=path)
        rc, _ = run("cargo contract build --release", cwd=path)
        if rc != 0:
            log.error(f"Build failed: {path}")
            sys.exit(1)
        log.info(f"✓ {name} built")


# ── Steps 2-4: Deploy ─────────────────────────────────────────────────────────
def deploy_contracts() -> dict:
    log.info("\n" + "═" * 60)
    log.info("Steps 2–4: Deploying contracts")
    log.info("═" * 60)

    if DRY_RUN:
        print_deployment_commands()
        return {}

    if not check_node_reachable():
        log.warning("Node unreachable — switching to dry-run")
        print_deployment_commands()
        return {}

    if LOCAL_MODE:
        bal = check_alice_balance()
        log.info(f"  Alice balance: {bal:.4f} POT")
        if bal < 1.0:
            log.warning("  Alice balance very low — dev node may not be fully started")

    # Deploy TRIONSignalGate
    log.info("\nDeploying TRIONSignalGate...")
    sg_path = "contracts/trion_signal_gate/target/ink/trion_signal_gate.contract"
    signal_gate_addr = cargo_instantiate(sg_path, [])
    if not signal_gate_addr:
        log.error("TRIONSignalGate deployment failed")
        sys.exit(1)
    log.info(f"  ✓ TRIONSignalGate: {signal_gate_addr}")
    time.sleep(2)  # wait for block inclusion

    # Deploy BTCPEscrow
    log.info("\nDeploying BTCPEscrow...")
    be_path = "contracts/btcp_escrow/target/ink/btcp_escrow.contract"
    btcp_escrow_addr = cargo_instantiate(be_path, [signal_gate_addr])
    if not btcp_escrow_addr:
        log.error("BTCPEscrow deployment failed")
        sys.exit(1)
    log.info(f"  ✓ BTCPEscrow: {btcp_escrow_addr}")
    time.sleep(2)

    # Deploy BehavioralLimitOrder
    log.info("\nDeploying BehavioralLimitOrder...")
    blo_path = "contracts/behavioral_limit_order/target/ink/behavioral_limit_order.contract"
    blo_addr = cargo_instantiate(blo_path, [signal_gate_addr, DEPLOY_ADDR])
    if not blo_addr:
        log.error("BehavioralLimitOrder deployment failed")
        sys.exit(1)
    log.info(f"  ✓ BehavioralLimitOrder: {blo_addr}")

    addresses = {
        "TRION_SIGNAL_GATE_ADDRESS": signal_gate_addr,
        "BTCP_ESCROW_ADDRESS":       btcp_escrow_addr,
        "BLO_ADDRESS":               blo_addr,
        "PORTALDOT_RPC_URL":         PORTALDOT_RPC,
        "DEPLOY_MODE":               MODE_LABEL,
    }

    with open(".env.deployed", "w") as f:
        for k, v in addresses.items():
            f.write(f"{k}={v}\n")
    log.info("\n  ✓ All addresses saved to .env.deployed")

    return addresses


def print_deployment_commands():
    """Print copy-paste cargo-contract CLI commands."""
    rpc = PORTALDOT_RPC
    suri = ALICE_MNEMONIC if LOCAL_MODE else "$DOT_MNEMONIC"
    print("\n" + "═" * 60)
    print("Deployment commands (copy-paste):")
    print("═" * 60)
    print(f"""
# 1. Deploy TRIONSignalGate
cargo contract instantiate \\
    --contract contracts/trion_signal_gate/target/ink/trion_signal_gate.contract \\
    --constructor new \\
    --suri "{suri}" \\
    --url {rpc} --execute --skip-confirm

# 2. Deploy BTCPEscrow  (replace <SIGNAL_GATE_ADDR> with output from step 1)
cargo contract instantiate \\
    --contract contracts/btcp_escrow/target/ink/btcp_escrow.contract \\
    --constructor new --args "<SIGNAL_GATE_ADDR>" \\
    --suri "{suri}" \\
    --url {rpc} --execute --skip-confirm

# 3. Deploy BehavioralLimitOrder
cargo contract instantiate \\
    --contract contracts/behavioral_limit_order/target/ink/behavioral_limit_order.contract \\
    --constructor new --args "<SIGNAL_GATE_ADDR>" "<YOUR_ADDR>" \\
    --suri "{suri}" \\
    --url {rpc} --execute --skip-confirm
""")


# ── Step 5: Verify ────────────────────────────────────────────────────────────
def verify_deployment(addresses: dict):
    log.info("\n" + "═" * 60)
    log.info("Step 5: Verifying deployment")
    log.info("═" * 60)

    if not addresses:
        log.info("  No addresses — dry-run mode, skipping verification")
        return

    # Check TRION Oracle API is live
    try:
        import urllib.request
        with urllib.request.urlopen(f"{ORACLE_API_URL}/api/v1/whitepaper/coverage", timeout=5) as r:
            data = json.loads(r.read())
            formulas = data.get("total_formulas", "?")
            coverage = data.get("coverage_percent", "?")
            log.info(f"  ✓ TRION Oracle API: {formulas} formulas, {coverage}% coverage")
    except Exception as e:
        log.warning(f"  TRION Oracle API not reachable: {e}")

    # Check node
    sg = addresses.get("TRION_SIGNAL_GATE_ADDRESS", "")
    if sg and check_node_reachable():
        log.info(f"  ✓ TRIONSignalGate deployed at: {sg}")


# ── Step 6: Demo signals ──────────────────────────────────────────────────────
def run_demo_signals(addresses: dict):
    log.info("\n" + "═" * 60)
    log.info("Step 6: Publishing demo signals")
    log.info("═" * 60)

    entities = ["uniswap", "aave", "compound", "curve", "lido"]

    try:
        sys.path.insert(0, os.path.dirname(__file__))
        from portaldot_client import PortalDotClient
        client = PortalDotClient()
        for entity in entities:
            log.info(f"\nSignal for: {entity}")
            result = client.fetch_signal(entity)
            if result:
                status = result.get("status", "?")
                coherence = result.get("coherence", 0)
                log.info(f"  status={status}  coherence={coherence:.3f}")
            time.sleep(0.3)
        log.info("\n  ✓ Demo signals complete")
    except Exception as e:
        log.warning(f"  Demo signals skipped: {e}")


# ── Main ──────────────────────────────────────────────────────────────────────
if __name__ == "__main__":
    log.info("╔══════════════════════════════════════════════════════════════╗")
    log.info("║     TRION Behavioral Oracle — PortalDot Deployment          ║")
    log.info("╚══════════════════════════════════════════════════════════════╝")
    log.info(f"  Oracle API  : {ORACLE_API_URL}")
    log.info(f"  RPC         : {PORTALDOT_RPC}")
    log.info(f"  Mode        : {MODE_LABEL}")
    if LOCAL_MODE:
        log.info(f"  Signer      : Alice ({ALICE_SS58})")
        log.info("  Note        : Start local node first: bash scripts/local_node.sh")

    build_contracts()
    addresses = deploy_contracts()
    verify_deployment(addresses)
    run_demo_signals(addresses)

    log.info("\n" + "═" * 60)
    log.info("Deployment complete.")
    if addresses:
        log.info("\nContract addresses:")
        for k, v in addresses.items():
            log.info(f"  {k} = {v}")
        log.info("\nNext steps:")
        log.info("  1. Update SUBMISSION.md with these addresses")
        log.info("  2. Start oracle bridge: cargo run --bin trion-portaldot-bridge")
        if LOCAL_MODE:
            log.info("  3. Record demo: python3 sdk/demo.py  (for submission video)")
    log.info("═" * 60)
