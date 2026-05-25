# PortalDot Hackathon 2026 — Official Submission Form

---

## 2.1 Public Demo Submission

| Field | Value |
|-------|-------|
| **Repository name** | `portaldot-hackathon-2026-trion-oracle-analys` |
| **Repository URL** | https://github.com/dev-analyshd/portaldot-hackathon-2026-trion-oracle-analys |
| **License** | MIT (see `LICENSE` file) |
| **Source code** | Full source + configuration included in root |

---

## 2.2 Submission Form

**Project Name:** TRION Behavioral Oracle for PortalDot

**Repository URL:** https://github.com/dev-analyshd/portaldot-hackathon-2026-trion-oracle-analys

**Demo Video URL:** `[FILL — record sdk/demo.py running, upload to YouTube/Loom]`

---

### Demo Scene Description
*(Max 200 words — objectively describe the core flow that reviewers can verify through the video)*

The demo runs `sdk/demo.py` against the live TRION Oracle API. Seven verifiable steps:

1. **API health check** — confirms 37 chains indexed, 84 whitepaper formulas live, 139 endpoints responding.
2. **Canonical behavioral hash** — calls `GET /api/v1/bh/{entity}` and displays the 93-byte dual-strand BH: `sense = SHA3(payload||0x00)` and `antisense = SHA3(payload||0xFF) ⊕ NOT(sense)`, matching whitepaper L0.1/L0.2.
3. **TRIONSignal (34 fields)** — calls `GET /api/v1/signal/{entity}` showing five-plane coherence `C(t) = α·Φ + β·M + γ·Σ + δ·K + ε·A`, dynamic threshold `Θ(t)`, and signal status for six entities.
4. **Manipulation fingerprints** — calls `GET /api/v1/security/{entity}/mf` displaying WASH/SYBIL/MEV/PUMP/FAKE_VOL scores (L1.2/L2.1–L2.6).
5. **Akashic Depth + TRION Score** — 37-chain behavioral consensus depth and Living Index tier.
6. **PortalDot ExecutionGate simulation** — calls `TRIONSignalGate.check_execution()` logic against six entities, showing ALLOWED vs BLOCKED with reason codes.
7. **Summary** — execution decisions printed to terminal; all contract addresses shown.

---

### Technical Highlights
*(Max 300 words)*

**Three Ink! v5 contracts on PortalDot (1,528 lines total, 14 unit tests):**

`TRIONSignalGate` (637 lines, 7 tests) is the core oracle gate. It stores a `BehavioralSignal` per entity keyed by 32-byte Universal Asset Identifier. Any PortalDot DeFi protocol calls `check_execution(entity_id)` pre-trade, receiving `(is_safe, status, phi_score, coherence)` in one call. The oracle-bridge Rust relay publishes signals on-chain via `publish_signal()` signed with SR25519 and POT gas. Signal has six states — SAFE and BOOTSTRAP permit execution; HOSTILE, COLLAPSE, ELEVATED, and SILENCE block it. SILENCE is emitted when five-plane coherence `C(t)` falls below dynamic threshold `Θ(t)` (Structured Silence, whitepaper L5.4) — the contract withholds judgement rather than emit a false signal.

`BTCPEscrow` (391 lines, 4 tests) is a two-state atomic escrow in POT. HOLDING → RELEASED if oracle confirms SAFE; HOLDING → REFUNDED if oracle confirms HOSTILE. Port of BTCPSimpleEscrow.sol from the BTCP Protocol to Ink!.

`BehavioralLimitOrder` (500 lines, 3 tests) matches orders by behavioral score: `BTCP_score = [0.25·NL + 0.20·gas_norm + 0.20·finality + 0.15·CC_coherence + 0.20·BEO] × (1 − MF_score)`. Counterparties with stronger 37-chain behavioral histories fill orders ahead of those with capital but no coherence history.

**Oracle Bridge** (Rust, 412 lines): polls TRION API every 60s, hand-encodes SCALE call data with the correct Ink! message selector, submits signed `contracts.call` extrinsic to PortalDot with SR25519 + DOT_MNEMONIC.

**Scale**: behavioral signals from 37 chains (35 mainnet), 13 VM families (EVM, SVM, Move, WASM, Cairo, UTXO, Cosmos SDK, TON, TVM, PI-MVM), 11,000–15,000+ FAISS behavioral vectors growing per block, 84 whitepaper formulas at 100% coverage. The CI pipeline (4 jobs) runs Ink! tests, bridge tests, Python validation, and produces WASM build artifacts on every push to `main`.

---

### Declaration

I confirm that:

1. All code was independently developed during this hackathon or legally modified from official Substrate/Ink! templates (three contracts written from scratch using Ink! v5.1.1 standard library only);
2. All delivery requirements of this specification have been met — public GitHub repo, correct naming format, README, LICENSE, full source code, configuration files;
3. I agree that the organizing committee may publicly review and technically reproduce the code.

**Signed:** Hudu Yusuf (Analys) — May 2026

---

## Reviewer Checklist

| Requirement | Status |
|-------------|--------|
| Repo name: `portaldot-hackathon-2026-[name]-[team]` | ✅ `portaldot-hackathon-2026-trion-oracle-analys` |
| Public GitHub repository | ✅ |
| `README.md` in root | ✅ 536 lines |
| `LICENSE` in root | ✅ MIT |
| Full source code | ✅ 18 files |
| Configuration files | ✅ `.env.example`, `Cargo.toml`, CI yml |
| Rust / Ink! / Substrate stack | ✅ Ink! v5.1.1, Substrate SR25519, SCALE |
| Deployed on PortalDot with POT gas | `[FILL — after deploying with funded wallet]` |
| Demo video | `[FILL — after recording sdk/demo.py]` |
