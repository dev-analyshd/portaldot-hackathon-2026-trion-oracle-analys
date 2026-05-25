# DoraHacks Submission — PortalDot Online Mini Hackathon S1

> Copy-paste this into the DoraHacks form. Fields marked `[FILL]` need your input after deployment.

---

## Project Name

**TRION Behavioral Oracle for PortalDot**

---

## One-Line Description

A behavioral truth oracle that queries 37 blockchains to determine whether an entity is acting coherently before any DeFi execution on PortalDot — deployed as three Ink! smart contracts with POT gas.

---

## Project Description (paste into DoraHacks form)

TRION is a multi-chain behavioral truth oracle — the ground truth layer (L0) beneath all DeFi activity.

Every DeFi protocol on PortalDot faces the same pre-execution blindness: they check token balances and slippage, but have no visibility into whether a counterparty is simultaneously wash-trading on Ethereum, coordinating MEV across Arbitrage and Base, or draining liquidity on five chains while depositing on PortalDot.

**TRION closes this gap.**

The system derives behavioral truth from the actual record of what every entity *did* across 37 chains (35 mainnet + 2 testnet), 13 VM families (EVM, SVM, WASM, Move, Cairo, UTXO, Cosmos SDK, TON, PI-MVM and more), stripped of manipulation noise, weighted by five-plane coherence, and bounded by liquidity health. 84 whitepaper formulas are live.

### Three Ink! Contracts Deployed on PortalDot

**1. TRIONSignalGate** — the oracle execution gate.
Any DeFi protocol calls `check_execution(entity_id)` before processing a trade. The contract returns `(is_safe, status, phi_score, coherence)` based on the entity's behavioral signal across 37 chains. The TRION relayer publishes signals on-chain via `publish_signal()` with POT gas.

Signal statuses: SAFE / ELEVATED (allow) | HOSTILE / COLLAPSE / SILENCE (block). SILENCE is emitted when five-plane coherence `C(t)` falls below dynamic threshold `Θ(t)` — the oracle withholds the signal rather than emit a false one (Structured Silence, whitepaper L5.4).

**2. BTCPEscrow** — behavioral escrow in POT.
Two-state atomic escrow: HOLDING → RELEASED (oracle=SAFE, POT to beneficiary) or REFUNDED (oracle=HOSTILE, POT back to originator). Port of BTCPSimpleEscrow.sol from the BTCP protocol to Ink!.

**3. BehavioralLimitOrder** — orders matched by BTCP_score, not speed.
```
BTCP_score = [0.25·NL + 0.20·normalize_gas + 0.20·finality_conf
             + 0.15·CC_coherence + 0.20·BEO_continuity] × (1 − MF_score)
```
The entity with the most coherent 37-chain behavioral history fills the order — not whoever arrives first, not whoever has the most capital.

### Oracle Bridge
A Rust binary polls the TRION Oracle API (37-chain behavioral consensus) every 60 seconds, encodes the result as SCALE call data, and submits it to TRIONSignalGate via a signed `contracts.call` extrinsic on PortalDot. The `DOT_MNEMONIC` signs with SR25519. Gas paid in POT.

### Why This Matters for PortalDot

PortalDot's Substrate + Ink! stack is the natural home for cross-chain behavioral data — a chain built for cross-chain interaction, receiving behavioral signals from 37 chains. Rust-native Ink! contracts share the same language as TRION's 13 L0 indexer crates. POT aligns economic incentives with behavioral truth.

The BTCP protocol insight: *"An asset does not cross a bridge. A behavioral fact does."*
With 37 chains, TRION eliminates `37 × 36 / 2 = 666` bridge pair dependencies.

---

## Technical Stack

| Layer | Technology |
|-------|-----------|
| Smart contracts | Ink! v5.1.1 (Rust, Substrate) |
| Chain | PortalDot (Substrate-based) |
| Gas token | POT |
| Oracle bridge | Rust (reqwest, tokio, sha3) |
| Python SDK | substrate-interface |
| Signing | SR25519 (DOT_MNEMONIC) |
| Off-chain oracle | TRION Oracle API (Flask, 139 routes) |
| Behavioral indexers | 13 Rust crates (trion-evm, trion-svm, trion-cosmos, ...) |
| Vector intelligence | FAISS (128-dim, 11,000–15,000+ vectors) |
| Chains indexed | 37 (Ethereum, Solana, Bitcoin, Cosmos, Near, TON, ...) |
| VM families | 13 (EVM, SVM, WASM, Move, Cairo, UTXO, TVM, ...) |
| Whitepaper formulas | 84 (100% live) |

---

## Deployed Contract Addresses

> [FILL] after running `bash scripts/deploy.sh` with your funded PortalDot wallet.

| Contract | Address |
|----------|---------|
| TRIONSignalGate | `[FILL — from deploy output]` |
| BTCPEscrow | `[FILL — from deploy output]` |
| BehavioralLimitOrder | `[FILL — from deploy output]` |
| Network | PortalDot mainnet / testnet |
| Deployer | `[FILL — your SS58 address]` |
| Deploy tx | `[FILL — from cargo contract instantiate output]` |

---

## GitHub Repository

**https://github.com/dev-analyshd/trion-portaldot-oracle**

- License: CC0 — this knowledge belongs to everyone
- Language: Rust (Ink! v5) + Python
- 18 files | 3 contracts | 14 unit tests | CI pipeline (4 jobs)

### Repository Structure

```
contracts/
  trion_signal_gate/src/lib.rs    — 637 lines, 7 unit tests, 16 messages
  btcp_escrow/src/lib.rs          — 391 lines, 4 unit tests,  7 messages
  behavioral_limit_order/src/lib.rs — 500 lines, 3 unit tests, 10 messages
oracle-bridge/src/main.rs         — 412 lines, 4 unit tests (Rust relay)
sdk/portaldot_client.py           — Python SDK (substrate-interface)
sdk/deploy.py                     — Full deployment automation
sdk/demo.py                       — Hackathon demo script
scripts/deploy.sh                 — One-command deploy
scripts/test.sh                   — Full test suite
.github/workflows/ci.yml          — 4-job CI (Ink! tests, bridge, Python, WASM)
```

---

## Demo Video

> [FILL] Link to your recorded demo video.

**To record:** Start the TRION Oracle API, then run:
```bash
python3 sdk/demo.py
```

The demo (≈45 seconds) shows:
1. TRION Oracle API live — 84 formulas, 100% coverage
2. L0.1 canonical behavioral hashes (sense + antisense strands)
3. L5.5 TRIONSignal — five-plane coherence, threshold, status for 6 entities
4. L1.2 manipulation fingerprints — WASH/SYBIL/MEV/PUMP/FAKE_VOL
5. L0.6 Akashic Depth + TRION Score — 37-chain consensus
6. PortalDot ExecutionGate simulation — `check_execution()` results
7. Summary — ALLOWED vs BLOCKED executions

---

## Running the Project (Judges)

### Option A — Test locally against the live oracle

```bash
git clone https://github.com/dev-analyshd/trion-portaldot-oracle
cd trion-portaldot-oracle

# Run unit tests (requires Rust + cargo)
cd contracts/trion_signal_gate    && cargo test --features std
cd ../btcp_escrow                 && cargo test --features std
cd ../behavioral_limit_order      && cargo test --features std
cd ../..                          && cargo test  # oracle bridge

# Run demo against live TRION API
ORACLE_API_URL=https://[FILL-TRION-URL] python3 sdk/demo.py
```

### Option B — Full deployment

```bash
# Prerequisites
cargo install cargo-contract --locked
pip install substrate-interface requests

# Set environment
export DOT_MNEMONIC="your twelve word mnemonic"
export PORTALDOT_RPC_URL="wss://rpc.portaldot.io"

# Build + deploy + demo
bash scripts/deploy.sh
```

### Option C — Query deployed contracts

```bash
# After deployment, query a signal
cargo contract call \
  --contract <SIGNAL_GATE_ADDRESS> \
  --message get_signal \
  --args <ENTITY_ID_HEX_32_BYTES> \
  --url wss://rpc.portaldot.io \
  --suri "$DOT_MNEMONIC"
```

---

## Team

| Field | Value |
|-------|-------|
| **Name** | Hudu Yusuf |
| **Handle** | Analys |
| **GitHub** | @dev-analyshd |
| **Role** | Solo — architect, protocol designer, engineer |
| **Background** | Designed TRION from first principles (37 chains, 84 formulas, 13 VM families). Author of BTCP Protocol. |
| **Contact** | [FILL — your email or Telegram] |

---

## Whitepaper Formulas Live in This Submission

| Formula | Description |
|---------|-------------|
| L0.1 | Canonical BH: `entity(32)\|\|event(1)\|\|mag(8)\|\|ctx(8)\|\|ts(8)\|\|chain(4)\|\|block_hash(32)` |
| L0.2 | Dual-strand: `sense=SHA3(payload\|\|0x00)`, `antisense=SHA3(payload\|\|0xFF)⊕NOT(sense)` |
| L0.6 | Akashic depth: `D(t) = Σ BH_count × chain_weight` |
| L0.7 | BTV: `P_ref × Ω × (1−MF) × C_weight × NL_weight` |
| L0.8 | Inverted Truth Hierarchy (Layer 0–4) |
| L1.2 | MF: `max(WASH, SYBIL, GOV_CAPTURE, MEV, PUMP, FAKE_VOL)` |
| L1.4 | Natural Liquidity: `NL = LD·LO·LC·LS` |
| L5.1 | Dynamic threshold: `Θ(t) = Θ_min + (Θ_max−Θ_min)·V(t)` |
| L5.2 | Five-plane coherence: `C(t) = α·Φ + β·M + γ·Σ + δ·K + ε·A` |
| L5.4 | Structured Silence: emission withheld when `C(t) < Θ(t)` |
| L6.1 | GK Evolution: `GK(t) = Hash_DNA(GK(t-1)\|\|BE(t)\|\|TM(t)\|\|CV(t))` |
| §4.2 | BTCP_score: `[0.25·NL + 0.20·gas + 0.20·finality + 0.15·CC + 0.20·BEO] × (1−MF)` |

---

## Related Work

- **TRION Oracle** (parent system) — 37-chain behavioral oracle, 84 formulas, 139 API routes, running
- **BTCP Protocol** — [github.com/dev-analyshd/btcp-protocol](https://github.com/dev-analyshd/btcp-protocol) — behavioral routing protocol whose Solidity contracts are ported to Ink! in this submission
- **0G Deployment** — TRIONOracleV3 deployed on 0G Galileo testnet (`0x0471B2BE25c2eBbAe7FAc17383F1692979F0A87C`)
- **NEAR Deployment** — trion.testnet, 304,895-byte WASM

---

## License

**CC0** — This knowledge belongs to everyone.

> *"The inverted truth hierarchy is not a product. It is a mirror."*
