//! TRION Signal Gate — Ink! oracle contract for PortalDot
//!
//! The behavioral ground truth layer for every DeFi protocol on PortalDot.
//! Any contract calls check_execution() before processing a trade.
//! Oracle publishers (TRION relayer) call publish_signal() to update state.
//!
//! Whitepaper formulas live: L0.1 BH, L1.2 MF, L5.2 C(t), L5.4 Structured Silence
//! Author: Hudu Yusuf (Analys) | CC0

#![cfg_attr(not(feature = "std"), no_std, no_main)]

#[ink::contract]
mod trion_signal_gate {
    use ink::prelude::string::String;
    use ink::prelude::vec::Vec;
    use ink::storage::Mapping;

    // ── Signal status (mirrors TRION Oracle API statuses) ─────────────────────
    #[derive(scale::Decode, scale::Encode, Clone, PartialEq, Debug)]
    #[cfg_attr(
        feature = "std",
        derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
    )]
    pub enum SignalStatus {
        /// φ(t) ≥ Θ(t) — execution fully allowed
        Safe,
        /// φ rising — execution cautioned but permitted
        Elevated,
        /// MEV / exploit pattern detected — BLOCKED
        Hostile,
        /// φ drop detected — BLOCKED
        Collapse,
        /// New entity: conf_genesis active, bootstrap decay applies
        Bootstrap,
        /// C(t) < Θ(t) — Structured Silence, emission withheld
        Silence,
    }

    // ── Full 34-field TRIONSignal (compressed for on-chain storage) ───────────
    #[derive(scale::Decode, scale::Encode, Clone, Debug)]
    #[cfg_attr(
        feature = "std",
        derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
    )]
    pub struct BehavioralSignal {
        /// Universal Asset Identifier: SHA3-256(chain_id||address||entity_type||genesis_block)
        pub entity_id: [u8; 32],
        /// Physical plane score × 1e9 (L1.1: Φ = Σ wᵢfᵢ)
        pub phi_score: u64,
        /// Five-plane coherence × 1e9 (L5.2: C(t) = α·Φ + β·M + γ·Σ + δ·K + ε·A)
        pub coherence: u64,
        /// Dynamic threshold × 1e9 (L5.1: Θ(t) = Θ_min + (Θ_max − Θ_min)·V(t))
        pub threshold: u64,
        /// Manipulation fingerprint score × 1e9 (L1.2: MF = max(WASH,SYBIL,GOV,MEV,PUMP,FAKE))
        pub mf_score: u64,
        /// Natural Liquidity score × 1e9 (L1.4: NL = LD·LO·LC·LS)
        pub nl_score: u64,
        /// Behavioral True Value discount × 1e9 (L0.7: BTV discount)
        pub btv_discount: u64,
        /// Execution gate status
        pub status: SignalStatus,
        /// Canonical 93-byte BH sense strand: SHA3-256(payload||0x00)
        pub behavioral_hash: [u8; 32],
        /// Antisense strand: SHA3-256(payload||0xFF) ⊕ NOT(sense) — tamper-evident
        pub antisense_hash: [u8; 32],
        /// Number of chains contributing to this signal (max 37)
        pub chain_count: u32,
        /// FAISS archetype index (0-63, 128-dim behavioral vector cluster)
        pub archetype: u8,
        /// Block number when signal was published on PortalDot
        pub published_block: u32,
        /// Unix timestamp of publication
        pub published_timestamp: u64,
        /// TTL in blocks — signal expires after this many blocks
        pub ttl_blocks: u32,
        /// Genomic Key (first 8 bytes): GK(t) = Hash_DNA(GK(t-1)||BE(t)||TM(t)||CV(t))
        pub genomic_key_prefix: [u8; 8],
    }

    // ── Escrow-linked route record (for BTCP integration) ────────────────────
    #[derive(scale::Decode, scale::Encode, Clone, Debug)]
    #[cfg_attr(
        feature = "std",
        derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
    )]
    pub struct RouteVerification {
        pub route_id: [u8; 32],
        pub is_safe: bool,
        pub coherence_out: u64,
        pub threshold_out: u64,
        pub verified_block: u32,
    }

    // ── Contract storage ──────────────────────────────────────────────────────
    #[ink(storage)]
    pub struct TRIONSignalGate {
        /// entity_id → latest BehavioralSignal
        signals: Mapping<[u8; 32], BehavioralSignal>,
        /// AccountId → entity_id (convenience mapping for DeFi protocols)
        account_to_entity: Mapping<AccountId, [u8; 32]>,
        /// route_id → RouteVerification (for BTCP escrow integration)
        route_verifications: Mapping<[u8; 32], RouteVerification>,
        /// Authorized oracle publishers (TRION relayer wallets)
        oracle_publishers: Mapping<AccountId, bool>,
        /// Contract owner
        owner: AccountId,
        /// TRION Oracle API URL (on-chain transparency)
        oracle_api_url: Vec<u8>,
        /// Total signals published
        total_signals: u64,
        /// Total executions blocked
        total_blocked: u64,
        /// Total executions allowed
        total_allowed: u64,
        /// Minimum coherence floor (safety invariant, scaled × 1e9)
        global_threshold_floor: u64,
        /// Akashic Depth D(t): cumulative BH count × chain_weight (L0.6)
        akashic_depth: u64,
    }

    // ── Events ────────────────────────────────────────────────────────────────
    #[ink(event)]
    pub struct SignalPublished {
        #[ink(topic)]
        entity_id: [u8; 32],
        #[ink(topic)]
        status: u8,
        phi_score: u64,
        coherence: u64,
        mf_score: u64,
        published_block: u32,
    }

    #[ink(event)]
    pub struct ExecutionBlocked {
        #[ink(topic)]
        entity_id: [u8; 32],
        caller: AccountId,
        status: u8,
        phi_score: u64,
        block_number: u32,
    }

    #[ink(event)]
    pub struct ExecutionAllowed {
        #[ink(topic)]
        entity_id: [u8; 32],
        caller: AccountId,
        coherence: u64,
        block_number: u32,
    }

    #[ink(event)]
    pub struct RouteVerified {
        #[ink(topic)]
        route_id: [u8; 32],
        is_safe: bool,
        coherence_out: u64,
    }

    #[ink(event)]
    pub struct OraclePublisherSet {
        publisher: AccountId,
        authorized: bool,
    }

    // ── Errors ────────────────────────────────────────────────────────────────
    #[derive(scale::Decode, scale::Encode, Debug, PartialEq)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum Error {
        NotOwner,
        NotAuthorizedPublisher,
        SignalNotFound,
        SignalExpired,
        EntityNotRegistered,
        ZeroEntityId,
        InvalidScore,
    }

    pub type Result<T> = core::result::Result<T, Error>;

    impl TRIONSignalGate {
        // ── Constructor ───────────────────────────────────────────────────────
        #[ink(constructor)]
        pub fn new(oracle_api_url: Vec<u8>) -> Self {
            let caller = Self::env().caller();
            let mut oracle_publishers = Mapping::default();
            oracle_publishers.insert(caller, &true);

            Self {
                signals: Mapping::default(),
                account_to_entity: Mapping::default(),
                route_verifications: Mapping::default(),
                oracle_publishers,
                owner: caller,
                oracle_api_url,
                total_signals: 0,
                total_blocked: 0,
                total_allowed: 0,
                global_threshold_floor: 300_000_000, // 0.30 × 1e9 — absolute floor
                akashic_depth: 0,
            }
        }

        // ── Oracle: publish behavioral signal ─────────────────────────────────
        /// Called by authorized TRION relayer after computing signal off-chain.
        /// Emits SignalPublished. Gas paid in POT.
        #[ink(message)]
        pub fn publish_signal(
            &mut self,
            entity_id: [u8; 32],
            phi_score: u64,
            coherence: u64,
            threshold: u64,
            mf_score: u64,
            nl_score: u64,
            btv_discount: u64,
            status_code: u8,
            behavioral_hash: [u8; 32],
            antisense_hash: [u8; 32],
            chain_count: u32,
            archetype: u8,
            ttl_blocks: u32,
            genomic_key_prefix: [u8; 8],
            akashic_depth_delta: u64,
        ) -> Result<()> {
            let caller = self.env().caller();
            if !self.oracle_publishers.get(caller).unwrap_or(false) {
                return Err(Error::NotAuthorizedPublisher);
            }
            if entity_id == [0u8; 32] {
                return Err(Error::ZeroEntityId);
            }

            let status = Self::decode_status(status_code);
            let block_number = self.env().block_number();
            let timestamp = self.env().block_timestamp();

            let signal = BehavioralSignal {
                entity_id,
                phi_score,
                coherence,
                threshold,
                mf_score,
                nl_score,
                btv_discount,
                status: status.clone(),
                behavioral_hash,
                antisense_hash,
                chain_count,
                archetype,
                published_block: block_number,
                published_timestamp: timestamp,
                ttl_blocks,
                genomic_key_prefix,
            };

            self.signals.insert(entity_id, &signal);
            self.total_signals = self.total_signals.saturating_add(1);
            self.akashic_depth = self.akashic_depth.saturating_add(akashic_depth_delta);

            self.env().emit_event(SignalPublished {
                entity_id,
                status: status_code,
                phi_score,
                coherence,
                mf_score,
                published_block: block_number,
            });

            Ok(())
        }

        // ── DeFi Gate: check execution for entity_id ──────────────────────────
        /// Primary gate function. Returns (is_safe, status_code, phi_score, coherence).
        /// Called by DeFi protocols BEFORE executing any transaction.
        /// L5.4 enforcement: emits ExecutionBlocked if C(t) < Θ(t).
        #[ink(message)]
        pub fn check_execution(
            &mut self,
            entity_id: [u8; 32],
        ) -> Result<(bool, u8, u64, u64)> {
            let signal = self.signals.get(entity_id).ok_or(Error::SignalNotFound)?;

            let current_block = self.env().block_number();
            if signal.ttl_blocks > 0
                && current_block > signal.published_block.saturating_add(signal.ttl_blocks)
            {
                return Err(Error::SignalExpired);
            }

            let status_code = Self::encode_status(&signal.status);
            let is_safe = matches!(
                signal.status,
                SignalStatus::Safe | SignalStatus::Bootstrap
            );

            let caller = self.env().caller();

            if is_safe {
                self.total_allowed = self.total_allowed.saturating_add(1);
                self.env().emit_event(ExecutionAllowed {
                    entity_id,
                    caller,
                    coherence: signal.coherence,
                    block_number: current_block,
                });
            } else {
                self.total_blocked = self.total_blocked.saturating_add(1);
                self.env().emit_event(ExecutionBlocked {
                    entity_id,
                    caller,
                    status: status_code,
                    phi_score: signal.phi_score,
                    block_number: current_block,
                });
            }

            Ok((is_safe, status_code, signal.phi_score, signal.coherence))
        }

        // ── DeFi Gate: check by AccountId (for EVM-style address lookup) ──────
        #[ink(message)]
        pub fn check_execution_by_account(
            &mut self,
            account: AccountId,
        ) -> Result<(bool, u8, u64, u64)> {
            let entity_id = self
                .account_to_entity
                .get(account)
                .ok_or(Error::EntityNotRegistered)?;
            self.check_execution(entity_id)
        }

        // ── BTCP: verify route execution (for BTCPEscrow integration) ─────────
        /// Matches verifyExecution() in ITRIONOracleV3 interface.
        /// Returns (is_safe, coherence_out, threshold_out).
        #[ink(message)]
        pub fn verify_execution(
            &mut self,
            route_id: [u8; 32],
        ) -> Result<(bool, u64, u64)> {
            let signal = self.signals.get(route_id).ok_or(Error::SignalNotFound)?;

            let current_block = self.env().block_number();
            if signal.ttl_blocks > 0
                && current_block > signal.published_block.saturating_add(signal.ttl_blocks)
            {
                return Err(Error::SignalExpired);
            }

            let is_safe = signal.coherence >= signal.threshold.max(self.global_threshold_floor);

            let verification = RouteVerification {
                route_id,
                is_safe,
                coherence_out: signal.coherence,
                threshold_out: signal.threshold,
                verified_block: current_block,
            };
            self.route_verifications.insert(route_id, &verification);

            self.env().emit_event(RouteVerified {
                route_id,
                is_safe,
                coherence_out: signal.coherence,
            });

            Ok((is_safe, signal.coherence, signal.threshold))
        }

        // ── Register account → entity_id mapping ─────────────────────────────
        #[ink(message)]
        pub fn register_account(
            &mut self,
            account: AccountId,
            entity_id: [u8; 32],
        ) -> Result<()> {
            let caller = self.env().caller();
            if !self.oracle_publishers.get(caller).unwrap_or(false) {
                return Err(Error::NotAuthorizedPublisher);
            }
            self.account_to_entity.insert(account, &entity_id);
            Ok(())
        }

        // ── Views ─────────────────────────────────────────────────────────────
        #[ink(message)]
        pub fn get_signal(&self, entity_id: [u8; 32]) -> Option<BehavioralSignal> {
            self.signals.get(entity_id)
        }

        #[ink(message)]
        pub fn get_signal_status(&self, entity_id: [u8; 32]) -> Option<u8> {
            self.signals
                .get(entity_id)
                .map(|s| Self::encode_status(&s.status))
        }

        #[ink(message)]
        pub fn is_safe(&self, entity_id: [u8; 32]) -> bool {
            self.signals
                .get(entity_id)
                .map(|s| matches!(s.status, SignalStatus::Safe | SignalStatus::Bootstrap))
                .unwrap_or(false)
        }

        #[ink(message)]
        pub fn get_stats(&self) -> (u64, u64, u64, u64) {
            (
                self.total_signals,
                self.total_allowed,
                self.total_blocked,
                self.akashic_depth,
            )
        }

        #[ink(message)]
        pub fn get_oracle_api_url(&self) -> Vec<u8> {
            self.oracle_api_url.clone()
        }

        #[ink(message)]
        pub fn get_global_threshold_floor(&self) -> u64 {
            self.global_threshold_floor
        }

        // ── Admin ─────────────────────────────────────────────────────────────
        #[ink(message)]
        pub fn set_oracle_publisher(&mut self, publisher: AccountId, authorized: bool) -> Result<()> {
            if self.env().caller() != self.owner {
                return Err(Error::NotOwner);
            }
            self.oracle_publishers.insert(publisher, &authorized);
            self.env().emit_event(OraclePublisherSet { publisher, authorized });
            Ok(())
        }

        #[ink(message)]
        pub fn set_threshold_floor(&mut self, floor: u64) -> Result<()> {
            if self.env().caller() != self.owner {
                return Err(Error::NotOwner);
            }
            self.global_threshold_floor = floor;
            Ok(())
        }

        #[ink(message)]
        pub fn set_oracle_api_url(&mut self, url: Vec<u8>) -> Result<()> {
            if self.env().caller() != self.owner {
                return Err(Error::NotOwner);
            }
            self.oracle_api_url = url;
            Ok(())
        }

        #[ink(message)]
        pub fn transfer_ownership(&mut self, new_owner: AccountId) -> Result<()> {
            if self.env().caller() != self.owner {
                return Err(Error::NotOwner);
            }
            self.owner = new_owner;
            Ok(())
        }

        #[ink(message)]
        pub fn owner(&self) -> AccountId {
            self.owner
        }

        // ── Internal helpers ──────────────────────────────────────────────────
        fn decode_status(code: u8) -> SignalStatus {
            match code {
                0 => SignalStatus::Safe,
                1 => SignalStatus::Elevated,
                2 => SignalStatus::Hostile,
                3 => SignalStatus::Collapse,
                4 => SignalStatus::Bootstrap,
                _ => SignalStatus::Silence,
            }
        }

        fn encode_status(status: &SignalStatus) -> u8 {
            match status {
                SignalStatus::Safe => 0,
                SignalStatus::Elevated => 1,
                SignalStatus::Hostile => 2,
                SignalStatus::Collapse => 3,
                SignalStatus::Bootstrap => 4,
                SignalStatus::Silence => 5,
            }
        }
    }

    // ── Tests ─────────────────────────────────────────────────────────────────
    #[cfg(test)]
    mod tests {
        use super::*;

        fn default_entity() -> [u8; 32] {
            [1u8; 32]
        }

        fn default_bh() -> [u8; 32] {
            [0xabu8; 32]
        }

        fn default_antisense() -> [u8; 32] {
            [0x54u8; 32]
        }

        #[ink::test]
        fn constructor_sets_owner() {
            let gate = TRIONSignalGate::new(b"https://trion-portaldot.oracle".to_vec());
            let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
            assert_eq!(gate.owner(), accounts.alice);
        }

        #[ink::test]
        fn publish_and_check_safe() {
            let mut gate = TRIONSignalGate::new(b"https://trion-portaldot.oracle".to_vec());
            let entity = default_entity();

            let result = gate.publish_signal(
                entity,
                750_000_000, // phi=0.75
                800_000_000, // coherence=0.80
                700_000_000, // threshold=0.70
                50_000_000,  // mf=0.05
                900_000_000, // nl=0.90
                203_000_000, // btv_discount=0.203
                0,           // Safe
                default_bh(),
                default_antisense(),
                37,
                12,
                1000,
                [0xab; 8],
                50_000,
            );
            assert!(result.is_ok());

            let (is_safe, status, phi, coherence) = gate.check_execution(entity).unwrap();
            assert!(is_safe);
            assert_eq!(status, 0); // Safe
            assert_eq!(phi, 750_000_000);
            assert_eq!(coherence, 800_000_000);
        }

        #[ink::test]
        fn publish_hostile_blocks_execution() {
            let mut gate = TRIONSignalGate::new(b"https://trion-portaldot.oracle".to_vec());
            let entity = default_entity();

            gate.publish_signal(
                entity,
                200_000_000, // phi=0.20 — low
                250_000_000, // coherence=0.25 — below threshold
                700_000_000, // threshold=0.70
                850_000_000, // mf=0.85 — PUMP detected
                300_000_000,
                350_000_000,
                2, // Hostile
                default_bh(),
                default_antisense(),
                37, 5, 1000, [0xab; 8], 50_000,
            ).unwrap();

            let (is_safe, status, _, _) = gate.check_execution(entity).unwrap();
            assert!(!is_safe);
            assert_eq!(status, 2); // Hostile
        }

        #[ink::test]
        fn unknown_entity_returns_not_found() {
            let mut gate = TRIONSignalGate::new(b"https://trion-portaldot.oracle".to_vec());
            let unknown = [99u8; 32];
            let result = gate.check_execution(unknown);
            assert_eq!(result, Err(Error::SignalNotFound));
        }

        #[ink::test]
        fn stats_track_correctly() {
            let mut gate = TRIONSignalGate::new(b"https://trion-portaldot.oracle".to_vec());
            let entity1 = [1u8; 32];
            let entity2 = [2u8; 32];

            gate.publish_signal(entity1, 750_000_000, 800_000_000, 700_000_000,
                50_000_000, 900_000_000, 203_000_000, 0,
                default_bh(), default_antisense(), 37, 12, 1000, [0xab; 8], 50_000).unwrap();

            gate.publish_signal(entity2, 200_000_000, 250_000_000, 700_000_000,
                850_000_000, 300_000_000, 350_000_000, 2,
                default_bh(), default_antisense(), 37, 5, 1000, [0xab; 8], 50_000).unwrap();

            gate.check_execution(entity1).unwrap();
            gate.check_execution(entity2).unwrap();

            let (total, allowed, blocked, _depth) = gate.get_stats();
            assert_eq!(total, 2);
            assert_eq!(allowed, 1);
            assert_eq!(blocked, 1);
        }

        #[ink::test]
        fn unauthorized_publisher_rejected() {
            let mut gate = TRIONSignalGate::new(b"https://trion-portaldot.oracle".to_vec());
            let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.bob);

            let result = gate.publish_signal(
                [1u8; 32], 750_000_000, 800_000_000, 700_000_000,
                50_000_000, 900_000_000, 203_000_000, 0,
                default_bh(), default_antisense(), 37, 12, 1000, [0xab; 8], 50_000,
            );
            assert_eq!(result, Err(Error::NotAuthorizedPublisher));
        }

        #[ink::test]
        fn verify_execution_route() {
            let mut gate = TRIONSignalGate::new(b"https://trion-portaldot.oracle".to_vec());
            let route_id = [7u8; 32];

            gate.publish_signal(
                route_id, 750_000_000, 800_000_000, 700_000_000,
                50_000_000, 900_000_000, 203_000_000, 0,
                default_bh(), default_antisense(), 37, 12, 1000, [0xab; 8], 50_000,
            ).unwrap();

            let (is_safe, coherence_out, threshold_out) =
                gate.verify_execution(route_id).unwrap();
            assert!(is_safe);
            assert_eq!(coherence_out, 800_000_000);
            assert_eq!(threshold_out, 700_000_000);
        }
    }
}
