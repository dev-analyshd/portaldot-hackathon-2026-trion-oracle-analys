"""
PortalDot SDK Client — TRION Oracle Bridge
Wraps substrate-interface to interact with TRIONSignalGate,
BTCPEscrow, and BehavioralLimitOrder Ink! contracts on PortalDot.

Usage:
    client = PortalDotClient()
    client.deploy_all_contracts()
    client.publish_signal("uniswap")
    result = client.check_execution(entity_id_bytes)

Author: Hudu Yusuf (Analys) | CC0
"""

import os
import json
import time
import hashlib
import requests
import logging
from dataclasses import dataclass
from typing import Optional, Tuple

log = logging.getLogger("trion.portaldot")

# ── Configuration ─────────────────────────────────────────────────────────────
PORTALDOT_RPC      = os.getenv("PORTALDOT_RPC_URL",      "wss://rpc.portaldot.io")
ORACLE_API_URL     = os.getenv("ORACLE_API_URL",          "http://127.0.0.1:5000")
SIGNAL_GATE_ADDR   = os.getenv("TRION_SIGNAL_GATE_ADDRESS", "")
ESCROW_ADDR        = os.getenv("BTCP_ESCROW_ADDRESS",       "")
BLO_ADDR           = os.getenv("BLO_CONTRACT_ADDRESS",      "")
DOT_MNEMONIC       = os.getenv("DOT_MNEMONIC",              "")
SIGNAL_TTL_BLOCKS  = int(os.getenv("SIGNAL_TTL_BLOCKS",    "500"))

# ── Signal status codes (must match Ink! contract) ────────────────────────────
STATUS_SAFE      = 0
STATUS_ELEVATED  = 1
STATUS_HOSTILE   = 2
STATUS_COLLAPSE  = 3
STATUS_BOOTSTRAP = 4
STATUS_SILENCE   = 5

STATUS_NAMES = {
    0: "SAFE", 1: "ELEVATED", 2: "HOSTILE",
    3: "COLLAPSE", 4: "BOOTSTRAP", 5: "SILENCE"
}

# ── Parsed signal ─────────────────────────────────────────────────────────────
@dataclass
class BehavioralSignal:
    entity_name:     str
    entity_id:       bytes          # 32 bytes UAI
    phi_score:       int            # × 1e9
    coherence:       int            # × 1e9
    threshold:       int            # × 1e9
    mf_score:        int            # × 1e9
    nl_score:        int            # × 1e9
    btv_discount:    int            # × 1e9
    status_code:     int
    behavioral_hash: bytes          # 32 bytes
    antisense_hash:  bytes          # 32 bytes
    chain_count:     int
    archetype:       int
    genomic_key_prefix: bytes       # 8 bytes
    akashic_depth_delta: int

    @property
    def status_name(self) -> str:
        return STATUS_NAMES.get(self.status_code, "UNKNOWN")

    @property
    def is_safe(self) -> bool:
        return self.status_code in (STATUS_SAFE, STATUS_BOOTSTRAP)

    def __str__(self):
        return (
            f"BehavioralSignal({self.entity_name} | "
            f"φ={self.phi_score/1e9:.4f} | "
            f"C(t)={self.coherence/1e9:.4f} | "
            f"Θ(t)={self.threshold/1e9:.4f} | "
            f"MF={self.mf_score/1e9:.4f} | "
            f"status={self.status_name} | "
            f"chains={self.chain_count})"
        )


# ── PortalDot Client ──────────────────────────────────────────────────────────
class PortalDotClient:
    """
    High-level client for the TRION PortalDot Oracle system.
    Wraps substrate-interface for Ink! contract calls.
    """

    def __init__(self):
        self._substrate = None
        self._keypair   = None
        self._session   = requests.Session()
        self._session.headers["Content-Type"] = "application/json"
        self._dry_run = not bool(DOT_MNEMONIC)

        if self._dry_run:
            log.warning("DOT_MNEMONIC not set — running in dry-run mode")
        else:
            self._init_substrate()

    # ── Substrate connection ──────────────────────────────────────────────────
    def _init_substrate(self):
        try:
            from substrateinterface import SubstrateInterface, Keypair
            self._substrate = SubstrateInterface(url=PORTALDOT_RPC)
            self._keypair   = Keypair.create_from_mnemonic(DOT_MNEMONIC)
            log.info(f"Connected to PortalDot: {PORTALDOT_RPC}")
            log.info(f"Relayer address: {self._keypair.ss58_address}")
        except ImportError:
            log.warning("substrate-interface not installed — install with: pip install substrate-interface")
        except Exception as e:
            log.error(f"Could not connect to PortalDot: {e}")

    # ── TRION Oracle API fetch ────────────────────────────────────────────────
    def fetch_signal(self, entity_id: str) -> Optional[BehavioralSignal]:
        """Fetch behavioral signal from TRION Oracle API."""
        url = f"{ORACLE_API_URL}/api/v1/signal/{entity_id}"
        try:
            resp = self._session.get(url, timeout=10)
            resp.raise_for_status()
            data = resp.json()
            return self._parse_signal(entity_id, data)
        except Exception as e:
            log.error(f"fetch_signal({entity_id}): {e}")
            return None

    def _parse_signal(self, entity_name: str, data: dict) -> BehavioralSignal:
        entity_id = self._hash_entity_id(entity_name)

        def to_fixed(val, default=0.0) -> int:
            return int(float(data.get(val, default) or default) * 1_000_000_000)

        status_str = str(data.get("status", "SILENCE")).upper()
        status_code = {
            "SAFE": 0, "ELEVATED": 1, "HOSTILE": 2,
            "COLLAPSE": 3, "BOOTSTRAP": 4
        }.get(status_str, 5)

        bh  = self._parse_hex32(data.get("behavioral_hash", ""))
        anti = self._parse_hex32(data.get("antisense_hash", ""))

        gk_hex = (data.get("genomic_key") or "")[:16]
        try:
            gk_bytes = bytes.fromhex(gk_hex.ljust(16, "0"))[:8]
        except ValueError:
            gk_bytes = b"\x00" * 8

        depth = int(data.get("akashic_depth", 0) or 0)

        return BehavioralSignal(
            entity_name     = entity_name,
            entity_id       = entity_id,
            phi_score       = to_fixed("phi"),
            coherence       = to_fixed("coherence"),
            threshold       = to_fixed("threshold", 0.72),
            mf_score        = to_fixed("mf_score"),
            nl_score        = to_fixed("nl_score"),
            btv_discount    = to_fixed("btv_discount"),
            status_code     = status_code,
            behavioral_hash = bh,
            antisense_hash  = anti,
            chain_count     = int(data.get("chain_count", 37) or 37),
            archetype       = int(data.get("archetype", 0) or 0),
            genomic_key_prefix  = gk_bytes,
            akashic_depth_delta = depth // 1000,
        )

    # ── Publish signal to PortalDot ───────────────────────────────────────────
    def publish_signal(self, entity_name: str) -> Optional[str]:
        """
        Fetch signal from TRION Oracle API and publish to
        TRIONSignalGate Ink! contract on PortalDot.
        Returns extrinsic hash (or None on failure).
        """
        signal = self.fetch_signal(entity_name)
        if not signal:
            return None

        log.info(f"Publishing: {signal}")

        if self._dry_run:
            log.info(f"  [DRY_RUN] Would call publish_signal on {SIGNAL_GATE_ADDR or '<not set>'}")
            return None

        return self._call_contract(
            contract_address = SIGNAL_GATE_ADDR,
            method           = "publish_signal",
            args             = self._encode_signal_args(signal),
        )

    def _encode_signal_args(self, s: BehavioralSignal) -> dict:
        return {
            "entity_id":           list(s.entity_id),
            "phi_score":           s.phi_score,
            "coherence":           s.coherence,
            "threshold":           s.threshold,
            "mf_score":            s.mf_score,
            "nl_score":            s.nl_score,
            "btv_discount":        s.btv_discount,
            "status_code":         s.status_code,
            "behavioral_hash":     list(s.behavioral_hash),
            "antisense_hash":      list(s.antisense_hash),
            "chain_count":         s.chain_count,
            "archetype":           s.archetype,
            "ttl_blocks":          SIGNAL_TTL_BLOCKS,
            "genomic_key_prefix":  list(s.genomic_key_prefix),
            "akashic_depth_delta": s.akashic_depth_delta,
        }

    # ── Check execution ───────────────────────────────────────────────────────
    def check_execution(self, entity_id_hex: str) -> Tuple[bool, int, int, int]:
        """
        Query TRIONSignalGate.check_execution() for an entity.
        Returns (is_safe, status_code, phi_score, coherence).
        """
        if not self._substrate or not SIGNAL_GATE_ADDR:
            # Fall back to TRION Oracle API
            signal = self.fetch_signal(entity_id_hex)
            if not signal:
                return (False, STATUS_SILENCE, 0, 0)
            return (signal.is_safe, signal.status_code, signal.phi_score, signal.coherence)

        result = self._query_contract(SIGNAL_GATE_ADDR, "check_execution", {
            "entity_id": self._parse_hex32_list(entity_id_hex)
        })
        if result:
            return (result[0], result[1], result[2], result[3])
        return (False, STATUS_SILENCE, 0, 0)

    # ── Deploy contracts ──────────────────────────────────────────────────────
    def deploy_signal_gate(self, oracle_api_url: str = ORACLE_API_URL) -> Optional[str]:
        """
        Deploy TRIONSignalGate Ink! contract to PortalDot.
        Returns deployed contract address.
        """
        if self._dry_run:
            log.info("[DRY_RUN] Would deploy TRIONSignalGate with:")
            log.info(f"  oracle_api_url = {oracle_api_url}")
            log.info("  Run: cargo contract build --release && cargo contract instantiate")
            return None

        return self._deploy_contract(
            wasm_path    = "contracts/trion_signal_gate/target/ink/trion_signal_gate.wasm",
            metadata_path= "contracts/trion_signal_gate/target/ink/trion_signal_gate.json",
            constructor  = "new",
            args         = {"oracle_api_url": list(oracle_api_url.encode())},
        )

    # ── Internal Substrate helpers ────────────────────────────────────────────
    def _call_contract(self, contract_address: str, method: str, args: dict) -> Optional[str]:
        if not self._substrate or not self._keypair:
            return None
        try:
            from substrateinterface.contracts import ContractInstance
            contract = ContractInstance.create_from_address(
                contract_address=contract_address,
                metadata_file=f"contracts/trion_signal_gate/target/ink/trion_signal_gate.json",
                substrate=self._substrate,
            )
            receipt = contract.exec(
                keypair    = self._keypair,
                method     = method,
                args       = args,
                value      = 0,
                gas_limit  = {"ref_time": 100_000_000_000, "proof_size": 1_000_000},
            )
            log.info(f"  Extrinsic: {receipt.extrinsic_hash}")
            return receipt.extrinsic_hash
        except Exception as e:
            log.error(f"_call_contract({method}): {e}")
            return None

    def _query_contract(self, contract_address: str, method: str, args: dict):
        if not self._substrate:
            return None
        try:
            from substrateinterface.contracts import ContractInstance
            contract = ContractInstance.create_from_address(
                contract_address=contract_address,
                metadata_file=f"contracts/trion_signal_gate/target/ink/trion_signal_gate.json",
                substrate=self._substrate,
            )
            result = contract.read(
                keypair = self._keypair,
                method  = method,
                args    = args,
            )
            return result.contract_result_data.value
        except Exception as e:
            log.error(f"_query_contract({method}): {e}")
            return None

    def _deploy_contract(self, wasm_path: str, metadata_path: str,
                         constructor: str, args: dict) -> Optional[str]:
        if not self._substrate or not self._keypair:
            return None
        try:
            from substrateinterface.contracts import ContractCode
            code = ContractCode.create_from_contract_files(
                metadata_file = metadata_path,
                wasm_file     = wasm_path,
                substrate     = self._substrate,
            )
            contract = code.deploy(
                keypair     = self._keypair,
                constructor = constructor,
                args        = args,
                value       = 0,
                gas_limit   = {"ref_time": 500_000_000_000, "proof_size": 10_000_000},
                upload_code = True,
            )
            log.info(f"Deployed to: {contract.contract_address}")
            return contract.contract_address
        except Exception as e:
            log.error(f"_deploy_contract: {e}")
            return None

    # ── Static helpers ────────────────────────────────────────────────────────
    @staticmethod
    def _hash_entity_id(entity: str) -> bytes:
        return hashlib.sha3_256(entity.encode()).digest()

    @staticmethod
    def _parse_hex32(hex_str: str) -> bytes:
        clean = (hex_str or "").lstrip("0x")
        try:
            b = bytes.fromhex(clean[:64].ljust(64, "0"))
        except ValueError:
            b = b"\x00" * 32
        return b[:32]

    @staticmethod
    def _parse_hex32_list(hex_str: str) -> list:
        return list(PortalDotClient._parse_hex32(hex_str))
