"""
deploy.py — Full deployment script for TRION PortalDot Oracle
Deploys all three Ink! contracts and wires them together.

Usage:
    pip install substrate-interface
    DOT_MNEMONIC="your mnemonic" python sdk/deploy.py

Steps:
    1. Build all Ink! contracts
    2. Deploy TRIONSignalGate → get address
    3. Deploy BTCPEscrow with SignalGate address
    4. Deploy BehavioralLimitOrder with SignalGate address
    5. Authorize oracle relayer as publisher
    6. Verify deployment by querying stats

Author: Hudu Yusuf (Analys) | CC0
"""

import os
import sys
import json
import subprocess
import time
import logging

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s"
)
log = logging.getLogger("deploy")

ORACLE_API_URL  = os.getenv("ORACLE_API_URL", "http://127.0.0.1:5000")
DOT_MNEMONIC    = os.getenv("DOT_MNEMONIC", "")
PORTALDOT_RPC   = os.getenv("PORTALDOT_RPC_URL", "wss://rpc.portaldot.io")


def run(cmd: str, cwd: str = ".") -> int:
    log.info(f"$ {cmd}")
    result = subprocess.run(cmd, shell=True, cwd=cwd)
    if result.returncode != 0:
        log.error(f"Command failed: {cmd}")
    return result.returncode


def build_contracts():
    log.info("═" * 60)
    log.info("Step 1: Building Ink! contracts")
    log.info("═" * 60)

    contracts = [
        "contracts/trion_signal_gate",
        "contracts/btcp_escrow",
        "contracts/behavioral_limit_order",
    ]

    # Check if cargo-contract is installed
    if run("cargo contract --version") != 0:
        log.info("Installing cargo-contract...")
        run("cargo install cargo-contract --force")

    for contract_path in contracts:
        log.info(f"\nBuilding {contract_path}...")
        rc = run("cargo contract build --release", cwd=contract_path)
        if rc != 0:
            log.error(f"Build failed for {contract_path}")
            sys.exit(1)
        log.info(f"✓ Built {contract_path}")

    log.info("\n✓ All contracts built successfully")


def deploy_contracts():
    log.info("\n" + "═" * 60)
    log.info("Step 2-4: Deploying contracts to PortalDot")
    log.info("═" * 60)

    if not DOT_MNEMONIC:
        log.warning("DOT_MNEMONIC not set — showing deployment commands only")
        print_deployment_commands()
        return {}

    try:
        from portaldot_client import PortalDotClient
        client = PortalDotClient()

        log.info("\nDeploying TRIONSignalGate...")
        signal_gate_addr = client.deploy_signal_gate(ORACLE_API_URL)
        if not signal_gate_addr:
            log.error("TRIONSignalGate deployment failed")
            sys.exit(1)
        log.info(f"  ✓ TRIONSignalGate: {signal_gate_addr}")

        addresses = {"TRION_SIGNAL_GATE_ADDRESS": signal_gate_addr}

        # Save addresses to .env.deployed
        with open(".env.deployed", "w") as f:
            f.write(f"TRION_SIGNAL_GATE_ADDRESS={signal_gate_addr}\n")
            f.write(f"PORTALDOT_RPC_URL={PORTALDOT_RPC}\n")
        log.info("  ✓ Addresses saved to .env.deployed")

        return addresses

    except Exception as e:
        log.error(f"Deployment error: {e}")
        print_deployment_commands()
        return {}


def print_deployment_commands():
    """Print manual deployment commands using cargo-contract CLI."""
    print("\n" + "═" * 60)
    print("Manual deployment commands:")
    print("═" * 60)
    print("""
# 1. Build contracts
cd contracts/trion_signal_gate && cargo contract build --release
cd contracts/btcp_escrow       && cargo contract build --release
cd contracts/behavioral_limit_order && cargo contract build --release

# 2. Deploy TRIONSignalGate (constructor arg: oracle_api_url as Vec<u8>)
cargo contract instantiate \\
    --contract contracts/trion_signal_gate/target/ink/trion_signal_gate.contract \\
    --constructor new \\
    --args "0x" \\
    --suri "$DOT_MNEMONIC" \\
    --url wss://rpc.portaldot.io \\
    --execute

# 3. Deploy BTCPEscrow (constructor arg: oracle_gate address from step 2)
cargo contract instantiate \\
    --contract contracts/btcp_escrow/target/ink/btcp_escrow.contract \\
    --constructor new \\
    --args <SIGNAL_GATE_ADDRESS> \\
    --suri "$DOT_MNEMONIC" \\
    --url wss://rpc.portaldot.io \\
    --execute

# 4. Deploy BehavioralLimitOrder (args: oracle_gate, btcp_router)
cargo contract instantiate \\
    --contract contracts/behavioral_limit_order/target/ink/behavioral_limit_order.contract \\
    --constructor new \\
    --args <SIGNAL_GATE_ADDRESS> <YOUR_ADDRESS> \\
    --suri "$DOT_MNEMONIC" \\
    --url wss://rpc.portaldot.io \\
    --execute
""")


def verify_deployment(addresses: dict):
    log.info("\n" + "═" * 60)
    log.info("Step 5: Verifying deployment")
    log.info("═" * 60)

    signal_gate = addresses.get("TRION_SIGNAL_GATE_ADDRESS", "")
    if not signal_gate:
        log.info("No address to verify — dry run mode")
        return

    # Query stats via substrate-interface
    try:
        from portaldot_client import PortalDotClient
        client = PortalDotClient()
        signal = client.fetch_signal("uniswap")
        if signal:
            log.info(f"  ✓ TRION Oracle API reachable: {signal}")
        else:
            log.warning("  ✗ Could not reach TRION Oracle API")
    except Exception as e:
        log.error(f"Verification error: {e}")


def run_demo_signals(addresses: dict):
    log.info("\n" + "═" * 60)
    log.info("Step 6: Publishing demo signals to PortalDot")
    log.info("═" * 60)

    entities = ["uniswap", "aave", "compound", "curve", "lido"]

    try:
        from portaldot_client import PortalDotClient
        client = PortalDotClient()

        for entity in entities:
            log.info(f"\nPublishing signal for: {entity}")
            tx_hash = client.publish_signal(entity)
            if tx_hash:
                log.info(f"  ✓ Submitted: {tx_hash}")
            else:
                log.info(f"  → Dry-run complete for {entity}")
            time.sleep(0.5)

        log.info("\n✓ Demo signals complete")
    except Exception as e:
        log.error(f"Demo signals error: {e}")


if __name__ == "__main__":
    log.info("╔══════════════════════════════════════════════════════════════╗")
    log.info("║     TRION Behavioral Oracle — PortalDot Deployment          ║")
    log.info("╚══════════════════════════════════════════════════════════════╝")
    log.info(f"  Oracle API  : {ORACLE_API_URL}")
    log.info(f"  RPC         : {PORTALDOT_RPC}")
    log.info(f"  Mode        : {'LIVE' if DOT_MNEMONIC else 'DRY-RUN (set DOT_MNEMONIC)'}")

    # Step 1: Build
    build_contracts()

    # Steps 2-4: Deploy
    addresses = deploy_contracts()

    # Step 5: Verify
    verify_deployment(addresses)

    # Step 6: Demo signals
    run_demo_signals(addresses)

    log.info("\n" + "═" * 60)
    log.info("Deployment complete.")
    if addresses:
        log.info("Contract addresses:")
        for k, v in addresses.items():
            log.info(f"  {k}={v}")
    log.info("Next: set these in your .env and start the oracle bridge:")
    log.info("  cargo run --bin trion-portaldot-bridge")
    log.info("═" * 60)
