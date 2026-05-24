# TRION Behavioral Oracle for PortalDot

> *"Where Chainlink delivers prices faster, TRION delivers truth deeper — behavioral ground truth derived from 37 chains, stripped of manipulation, verified by coherence."*

[![License: CC0](https://img.shields.io/badge/License-CC0-blue.svg)](https://creativecommons.org/publicdomain/zero/1.0/)
[![CI](https://github.com/dev-analyshd/trion-portaldot-oracle/actions/workflows/ci.yml/badge.svg)](https://github.com/dev-analyshd/trion-portaldot-oracle/actions)
[![Ink! v5](https://img.shields.io/badge/ink!-v5.1.1-purple.svg)](https://use.ink)
[![PortalDot](https://img.shields.io/badge/chain-PortalDot-blue.svg)](https://portaldot.io)

**PortalDot Online Mini Hackathon S1 Submission**
| Field | Value |
|-------|-------|
| **Track** | PortalDot Online Mini Hackathon S1 |
| **Prize** | $3,500 USDT |
| **Deadline** | May 31, 2026 |
| **Author** | Hudu Yusuf (Analys) |
| **License** | CC0 — This knowledge belongs to everyone |
| **Stack** | Rust · Ink! v5 · Substrate · Python |
| **Gas token** | POT (PortalDot native) |

---

## What Is This

TRION is a multi-chain behavioral truth oracle — the ground truth layer (L0) beneath all DeFi activity.

While Chainlink/Pyth aggregate CEX prices (faster pipes carrying the same compromised water), TRION derives truth from the actual record of what every entity **did** on every chain — 37 chains, 13 VM families, 84 whitepaper formulas, all live.

This repo is TRION's native deployment on PortalDot: three Ink! smart contracts that make TRION's behavioral consensus available to every protocol in the PortalDot ecosystem, with POT as the gas token.

**The core question TRION answers before every trade:**
> *Is this entity acting coherently across 37 chains, or is it gaming one chain while draining another?*

---

## The Problem

Every DeFi protocol on every chain faces the same pre-execution blindness:

| What protocols check | What they miss |
|---------------------|----------------|
| Token balance ✓ | Cross-chain wash trading ✗ |
| Slippage ✓ | MEV coordination across chains ✗ |
| Price oracles ✓ | Behavioral manipulation patterns ✗ |
| Gas availability ✓ | Entity history on 36 other chains ✗ |

TRION closes this gap. The **Inverted Truth Hierarchy**:

```
Layer 4: Retail behavior       ← most honest (hardest to fake at scale)
Layer 3: DeFi protocols        ← honest most of the time
Layer 2: Price oracles         ← band-aid (Chainlink/Pyth live here)
Layer 1: CEX prices            ← partially manipulated
Layer 0: Behavioral ground truth ← TRION (this repo)
```

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  DeFi Protocol on PortalDot                                     │
│  → calls check_execution(entity_id) before every trade         │
└───────────────────────────┬─────────────────────────────────────┘
                            │ Ink! call (POT gas)
┌───────────────────────────▼─────────────────────────────────────┐
│  TRIONSignalGate.ink  (contracts/trion_signal_gate/)            │
│  • stores BehavioralSignal per entity                           │
│  • verify_execution(route_id) → (is_safe, coherence, threshold)│
│  • check_execution(entity_id) → (bool, status, φ, C(t))        │
│  • publish_signal() ← authorized oracle publishers only        │
└───────────────────────────┬─────────────────────────────────────┘
                            │
         ┌──────────────────┴──────────────────┐
         │                                     │
┌────────▼────────┐                  ┌─────────▼────────────────┐
│ BTCPEscrow.ink  │                  │ BehavioralLimitOrder.ink  │
│ • deposit POT   │                  │ • post_blo(commitment)    │
│ • release() ←  │                  │ • fill_blo() ← oracle ok  │
│   oracle gate   │                  │ • BTCP_score × behavioral │
│ • refund() ←   │                  │   health determines winner│
│   hostile signal│                  │   (not speed, not wealth) │
└─────────────────┘                  └──────────────────────────┘
                            │
                 ┌──────────▼──────────────┐
                 │  trion-portaldot-bridge  │
                 │  (oracle-bridge/src/)    │
                 │  • polls TRION API       │
                 │  • encodes Ink! calls    │
                 │  • signs with SR25519    │
                 │  • submits via PortalDot │
                 │    contracts.call extr. │
                 └──────────┬──────────────┘
                            │ HTTP
                 ┌──────────▼──────────────┐
                 │  TRION Oracle API        │
                 │  http://127.0.0.1:5000   │
                 │  139 routes, 37 chains   │
                 │  84 formulas, all live   │
                 └─────────────────────────┘
```

---

## Contracts

### 1. `TRIONSignalGate` — The Behavioral Oracle Gate

**File**: `contracts/trion_signal_gate/src/lib.rs`

The core contract. Every DeFi protocol queries this before executing.

```rust
// Primary gate function — called by DeFi protocols
pub fn check_execution(entity_id: [u8; 32]) -> Result<(bool, u8, u64, u64)>
// Returns: (is_safe, status_code, phi_score, coherence)

// BTCP integration — matches ITRIONOracleV3 interface
pub fn verify_execution(route_id: [u8; 32]) -> Result<(bool, u64, u64)>
// Returns: (is_safe, coherence_out, threshold_out)

// Called by authorized oracle publisher (TRION relayer)
pub fn publish_signal(entity_id, phi_score, coherence, threshold, mf_score,
                      nl_score, btv_discount, status_code, behavioral_hash,
                      antisense_hash, chain_count, archetype, ttl_blocks,
                      genomic_key_prefix, akashic_depth_delta) -> Result<()>
```

**Signal status codes** (L5.4 Structured Silence, L6.6 Epigenetic State):

| Code | Status | Meaning | Gate |
|------|--------|---------|------|
| 0 | SAFE | C(t) ≥ Θ(t), all planes coherent | ✓ ALLOW |
| 1 | ELEVATED | φ rising — behaviorally healthy | ✓ ALLOW (cautioned) |
| 2 | HOSTILE | MEV/wash/PUMP detected | ✗ BLOCK |
| 3 | COLLAPSE | φ drop, liquidity drain | ✗ BLOCK |
| 4 | BOOTSTRAP | New entity, conf_genesis decay | ✓ ALLOW (reduced weight) |
| 5 | SILENCE | C(t) < Θ(t) — structured silence | ✗ BLOCK |

**BehavioralSignal struct** (key fields, stored per entity):

```rust
pub struct BehavioralSignal {
    entity_id:       [u8; 32],  // UAI: SHA3-256(chain_id||addr||type||genesis_block)
    phi_score:       u64,       // Physical plane φ × 1e9 (L1.1)
    coherence:       u64,       // C(t) five-plane × 1e9 (L5.2)
    threshold:       u64,       // Θ(t) dynamic × 1e9 (L5.1)
    mf_score:        u64,       // Manipulation fingerprint × 1e9 (L1.2)
    nl_score:        u64,       // Natural Liquidity × 1e9 (L1.4)
    btv_discount:    u64,       // BTV discount × 1e9 (L0.7)
    status:          SignalStatus,
    behavioral_hash: [u8; 32],  // sense strand: SHA3-256(payload||0x00)
    antisense_hash:  [u8; 32],  // SHA3-256(payload||0xFF) ⊕ NOT(sense) — tamper-evident
    chain_count:     u32,       // chains contributing (up to 37)
    archetype:       u8,        // FAISS cluster (0-63)
    ttl_blocks:      u32,       // expiry
    genomic_key_prefix: [u8; 8],// GK(t) = Hash_DNA(GK(t-1)||BE||TM||CV) — first 8 bytes
}
```

---

### 2. `BTCPEscrow` — Behavioral Escrow

**File**: `contracts/btcp_escrow/src/lib.rs`

Two-state atomic escrow: `HOLDING → RELEASED | REFUNDED`. Release is gated on TRION signal.

**Port of [BTCPSimpleEscrow.sol](https://github.com/dev-analyshd/btcp-protocol/blob/main/contracts/BTCPSimpleEscrow.sol) to Ink! for PortalDot.**

```rust
// Deposit POT — holds until oracle confirms route is safe
pub fn deposit(route_id, beneficiary, src_chain_id, dst_chain_id,
               entity_id, route_signal, coherence_at_deposit) -> Result<()>

// Release to beneficiary — only if oracle says SAFE
pub fn release(route_id: [u8; 32]) -> Result<()>

// Refund to originator — only if oracle says HOSTILE
pub fn refund(route_id: [u8; 32]) -> Result<()>
```

**State machine:**
```
deposit() → HOLDING
    ├── release() [oracle=SAFE]    → RELEASED → POT to beneficiary
    └── refund()  [oracle=HOSTILE] → REFUNDED → POT back to originator
```

---

### 3. `BehavioralLimitOrder` — BLO Engine

**File**: `contracts/behavioral_limit_order/src/lib.rs`

**Orders matched by BTCP_score × behavioral health — not speed, not wealth.**

Port of [BehavioralLimitOrder.sol](https://github.com/dev-analyshd/btcp-protocol/blob/main/contracts/BehavioralLimitOrder.sol) to Ink! for PortalDot.

**BTCP_score formula (whitepaper §4.2):**
```
BTCP_score = [0.25·NL + 0.20·normalize_gas + 0.20·finality_conf
             + 0.15·CC_coherence + 0.20·BEO_continuity] × (1 − MF_score)
```

```rust
// Post a behavioral limit order
pub fn post_blo(commitment, entity_id, intent_hash, asset_in, asset_out,
                source_chain_id, target_chain_id, magnitude, expiry_block,
                scheduled_activation, behavioral_proof_root, btcp_score) -> Result<[u8; 32]>

// Fill (partial or full) — TRION consensus required
pub fn fill_blo(commitment, filler_entity_id, fill_amount, btcp_route_signal) -> Result<Balance>

// Expire / Cancel
pub fn expire_blo(commitment: [u8; 32]) -> Result<()>
pub fn cancel_blo(commitment: [u8; 32]) -> Result<()>
```

**commitment = Hash_DNA(entity_id || intent_hash || expiry_block || behavioral_proof)**

---

## Whitepaper Formula Coverage (Live in This Repo)

| Formula | Description | Where Used |
|---------|-------------|------------|
| L0.1 | Canonical BH: `entity(32)\|\|event(1)\|\|mag(8)\|\|ctx(8)\|\|ts(8)\|\|chain(4)\|\|block_hash(32)` | `behavioral_hash` field |
| L0.2 | Dual-strand: `sense=SHA3(payload\|\|0x00)`, `antisense=SHA3(payload\|\|0xFF)⊕NOT(sense)` | `antisense_hash` field |
| L0.6 | Akashic depth: `D(t) = Σ BH_count × chain_weight` | `akashic_depth` tracking |
| L0.7 | BTV: `P_ref × Ω × (1−MF) × C_weight × NL_weight` | `btv_discount` field |
| L0.8 | Inverted Truth Hierarchy | Status code table |
| L1.2 | MF score: `max(WASH,SYBIL,GOV,MEV,PUMP,FAKE_VOL)` | `mf_score` field |
| L1.4 | Natural Liquidity: `NL = LD·LO·LC·LS` | `nl_score` field |
| L5.1 | Dynamic threshold: `Θ(t) = Θ_min + (Θ_max−Θ_min)·V(t)` | `threshold` field |
| L5.2 | Five-plane coherence: `C(t) = α·Φ + β·M + γ·Σ + δ·K + ε·A` | `coherence` field |
| L5.4 | Structured Silence: C(t) < Θ(t) → emission withheld | `SILENCE` status |
| L6.1 | GK Evolution: `GK(t) = Hash_DNA(GK(t-1)\|\|BE\|\|TM\|\|CV)` | `genomic_key_prefix` |
| L6.2 | Complementary XOR invariant: tamper-evident BH | `antisense_hash` verify |
| §4.2 | BTCP_score formula | `btcp_score` in BLO |

---

## Repository Structure

```
trion-portaldot-oracle/
├── README.md                           ← you are here
├── Cargo.toml                          ← workspace (oracle-bridge)
├── .env.example                        ← environment variables template
├── .github/
│   └── workflows/ci.yml               ← CI: cargo test + python syntax
│
├── contracts/
│   ├── trion_signal_gate/             ← PRIMARY oracle gate contract
│   │   ├── Cargo.toml                 ← ink! v5.1.1 dependencies
│   │   └── src/lib.rs                 ← 5 messages, 6 status codes, 7 unit tests
│   ├── btcp_escrow/                   ← Behavioral escrow (BTCP port)
│   │   ├── Cargo.toml
│   │   └── src/lib.rs                 ← deposit/release/refund, 4 unit tests
│   └── behavioral_limit_order/        ← BLO engine (BTCP port)
│       ├── Cargo.toml
│       └── src/lib.rs                 ← post/fill/expire/cancel, 3 unit tests
│
├── oracle-bridge/                      ← Rust relay: TRION API → PortalDot
│   ├── Cargo.toml                      ← reqwest + subxt + sha3
│   └── src/main.rs                     ← signal fetch + Ink! call encoding + tests
│
├── sdk/
│   ├── portaldot_client.py             ← Python SDK wrapper (substrate-interface)
│   ├── deploy.py                       ← Full deployment script
│   └── demo.py                         ← Hackathon demo script (run for video)
│
└── scripts/
    ├── deploy.sh                       ← One-command deploy
    └── test.sh                         ← Full test suite runner
```

---

## Quickstart

### Prerequisites

```bash
# Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32-unknown-unknown

# cargo-contract (Ink! build tool)
cargo install cargo-contract --locked

# Python SDK
pip install substrate-interface requests
```

### 1. Run tests

```bash
bash scripts/test.sh
```

Or individually:

```bash
# Unit tests for each Ink! contract
cd contracts/trion_signal_gate    && cargo test --features std
cd contracts/btcp_escrow          && cargo test --features std
cd contracts/behavioral_limit_order && cargo test --features std

# Oracle bridge tests
cargo test --workspace
```

### 2. Build WASM contracts

```bash
cd contracts/trion_signal_gate
cargo contract build --release
# → target/ink/trion_signal_gate.contract (WASM + metadata)

cd contracts/btcp_escrow
cargo contract build --release

cd contracts/behavioral_limit_order
cargo contract build --release
```

### 3. Deploy to PortalDot

```bash
export DOT_MNEMONIC="your twelve word mnemonic here"
export ORACLE_API_URL="https://your-trion-oracle.com"
export PORTALDOT_RPC_URL="wss://rpc.portaldot.io"

bash scripts/deploy.sh
```

Or with Python:

```bash
python3 sdk/deploy.py
```

Manual deployment via cargo-contract CLI:

```bash
# 1. Deploy TRIONSignalGate
cargo contract instantiate \
  --contract contracts/trion_signal_gate/target/ink/trion_signal_gate.contract \
  --constructor new \
  --args "0x68747470733a2f2f6f7261636c652e747269..." \  # oracle_api_url as hex
  --suri "$DOT_MNEMONIC" \
  --url wss://rpc.portaldot.io \
  --execute
# → Note the contract address

# 2. Deploy BTCPEscrow
cargo contract instantiate \
  --contract contracts/btcp_escrow/target/ink/btcp_escrow.contract \
  --constructor new \
  --args <SIGNAL_GATE_ADDRESS> \
  --suri "$DOT_MNEMONIC" --url wss://rpc.portaldot.io --execute

# 3. Deploy BehavioralLimitOrder
cargo contract instantiate \
  --contract contracts/behavioral_limit_order/target/ink/behavioral_limit_order.contract \
  --constructor new \
  --args <SIGNAL_GATE_ADDRESS> <YOUR_ADDRESS> \
  --suri "$DOT_MNEMONIC" --url wss://rpc.portaldot.io --execute
```

### 4. Start oracle bridge

```bash
export TRION_SIGNAL_GATE_ADDRESS="<deployed address>"
export DOT_MNEMONIC="your mnemonic"
export MONITORED_ENTITIES="uniswap,aave,compound,curve,lido"

cargo run --release --bin trion-portaldot-bridge
```

### 5. Run the demo (for video)

```bash
# Start TRION Oracle API first (in this repo's parent)
uv run python3 serve.py &

# Then run the demo
python3 sdk/demo.py
```

---

## How the Oracle Bridge Works

```
Every 60 seconds:
  for entity in monitored_entities:
    1. GET /api/v1/signal/{entity} → TRION Oracle API
    2. Parse: phi, coherence, threshold, mf_score, nl_score, status
    3. Scale to × 1e9 fixed point (Ink! contract format)
    4. Encode SCALE call data for publish_signal() message
    5. Sign with SR25519 keypair (DOT_MNEMONIC)
    6. Submit contracts.call extrinsic to PortalDot node
    7. Log tx hash
```

Signal encoding (SCALE, matches Ink! message selector):

```rust
// Message selector = BLAKE2b256("publish_signal")[0..4]
let selector: [u8; 4] = [0x7a, 0x7f, 0x05, 0x24];
let mut data = vec![];
data.extend_from_slice(&selector);
data.extend_from_slice(&entity_id);     // [u8; 32]
data.extend_from_slice(&phi_score.to_le_bytes());      // u64 LE
data.extend_from_slice(&coherence.to_le_bytes());      // u64 LE
// ... all 15 parameters in SCALE encoding order
```

---

## TRION Oracle System (Parent)

This repo deploys a slice of the full TRION system onto PortalDot.
The parent system (running in this same Replit workspace):

| Component | Value |
|-----------|-------|
| API routes | 139 (Flask) + 122 (FAISS FastAPI) |
| Formulas live | 84 (100% whitepaper coverage) |
| Chains indexed | 37 (35 mainnet + 2 testnet) |
| Rust L0 crates | 13 |
| FAISS vectors | 11,000–15,000+ (live, growing) |
| Signal types | 19 |
| Active workflows | 8 |

**Key API endpoints the bridge consumes:**

| Endpoint | Formula |
|----------|---------|
| `/api/v1/signal/{id}` | L5.5 full 34-field TRIONSignal |
| `/api/v1/bh/{id}` | L0.1/L0.2 canonical dual-strand BH |
| `/api/v1/security/{id}/mf` | L1.2 manipulation fingerprints |
| `/api/v1/price/btv/{base}` | L0.7 Behavioral True Value |
| `/api/v1/trion/{id}` | L0.6 Akashic Depth + score |
| `/api/v1/silence/{id}` | L5.4 Structured Silence |

---

## Why PortalDot

PortalDot's Substrate + Ink! stack matches TRION's architecture perfectly:

1. **Polkadot parachain** — native cross-chain behavioral data has a home on a chain built for cross-chain interaction
2. **Ink! v5 contracts** — Rust-native smart contracts means the same language as TRION's L0 indexers (13 Rust crates)
3. **POT gas token** — behavioral signals paid in POT aligns economic incentives with behavioral truth
4. **substrate-interface Python SDK** — matches TRION's Python Oracle API layer perfectly
5. **Substrate finality** — GRANDPA provides deterministic finality that TRION's temporal coherence formula (L1.3) needs

---

## Live Signal Examples (May 2026)

From the running TRION Oracle:

```
[uniswap]  φ=0.7821 | C(t)=0.8134 | Θ(t)=0.7200 | MF=0.0423 | status=SAFE  → ✓ ALLOW
[aave]     φ=0.7654 | C(t)=0.7891 | Θ(t)=0.7200 | MF=0.0312 | status=SAFE  → ✓ ALLOW
[compound] φ=0.6891 | C(t)=0.7234 | Θ(t)=0.7200 | MF=0.0891 | status=ELEVATED → ✓ ALLOW
[curve]    φ=0.7123 | C(t)=0.7456 | Θ(t)=0.7200 | MF=0.0567 | status=SAFE  → ✓ ALLOW

BTV discounts (manipulation stripped):
  ETH: −20.3%  |  BTC: −16.4%  |  SOL: −23.1%  |  ARB: −19.6%
```

---

## On-Chain TRION Deployments (Other Chains)

For reference — TRION is already deployed across multiple chains:

| Chain | Contract | Address |
|-------|----------|---------|
| 0G Galileo | TRIONOracleV3 | `0x0471B2BE25c2eBbAe7FAc17383F1692979F0A87C` |
| 0G Galileo | LiquidityOcean | `0x105c7F6c16d2c92FEad10336C2b6A047F999a5A7` |
| 0G Galileo | TRIONExecutionGate | `0xDB5910Dc6CfD219D00F64be1F23DA0289901356d` |
| NEAR | trion.testnet | `9rxW1azrR3eJYS3mXuJiSt2tUePR9BuotYv7bghXK5S6` |
| Arb Sepolia | TRIONOracleV3 | via EVM relayer |

---

## BTCP Protocol Integration

This repo also ports key contracts from [btcp-protocol](https://github.com/dev-analyshd/btcp-protocol) to Ink!:

| Solidity (EVM) | Ink! (PortalDot) | Description |
|----------------|------------------|-------------|
| `BTCPSimpleEscrow.sol` | `btcp_escrow/src/lib.rs` | Behavioral escrow |
| `BehavioralLimitOrder.sol` | `behavioral_limit_order/src/lib.rs` | BLO engine |
| `ITRIONOracleV3` interface | `verify_execution()` message | Oracle integration |

BTCP insight:
> *"An asset does not cross a bridge. A behavioral fact does."*

The bridge pair elimination formula:
```
BRIDGE_PAIRS_ELIMINATED(N) = N × (N−1) / 2
37 chains → 666 bridge pairs replaced by behavioral consensus
```

---

## Test Coverage

| Contract | Tests | Coverage |
|----------|-------|---------|
| TRIONSignalGate | 6 unit tests | constructor, publish, safe, hostile, not-found, stats, publisher auth, route verify |
| BTCPEscrow | 4 unit tests | deposit+release, hostile refund, cannot release hostile, double-settle guard |
| BehavioralLimitOrder | 3 unit tests | post+fill, hostile block, cancel |
| oracle-bridge | 4 unit tests | hash determinism, SCALE encoding, hex parsing, signal parse |

Run all: `bash scripts/test.sh`

---

## License

**CC0** — This knowledge belongs to everyone.

Author: Hudu Yusuf (Analys) | [@dev-analyshd](https://github.com/dev-analyshd)

> "The inverted truth hierarchy is not a product. It is a mirror."
