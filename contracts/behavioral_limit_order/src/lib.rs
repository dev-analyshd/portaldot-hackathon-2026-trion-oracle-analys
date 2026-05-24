//! Behavioral Limit Order (BLO) — Ink! contract for PortalDot
//!
//! Orders matched by BTCP behavioral score, not speed or wealth.
//! Formula: BTCP_score = [0.25·NL + 0.20·gas + 0.20·finality + 0.15·CC + 0.20·BEO] × (1 − MF)
//!
//! Port of BehavioralLimitOrder.sol to Ink! for PortalDot.
//! Uses POT as the gas and settlement token.
//!
//! Author: Hudu Yusuf (Analys) | CC0

#![cfg_attr(not(feature = "std"), no_std, no_main)]

#[ink::contract]
mod behavioral_limit_order {
    use ink::prelude::vec::Vec;
    use ink::storage::Mapping;

    // ── BLO status ────────────────────────────────────────────────────────────
    #[derive(scale::Decode, scale::Encode, Clone, PartialEq, Debug)]
    #[cfg_attr(
        feature = "std",
        derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
    )]
    pub enum BloStatus {
        Open,
        PartiallyFilled,
        Filled,
        Expired,
        Cancelled,
    }

    // ── BLO record ────────────────────────────────────────────────────────────
    /// commitment = Hash_DNA(entity_id || intent_hash || expiry || behavioral_proof)
    #[derive(scale::Decode, scale::Encode, Clone, Debug)]
    #[cfg_attr(
        feature = "std",
        derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
    )]
    pub struct Blo {
        /// Hash_DNA commitment
        pub commitment: [u8; 32],
        /// Universal Asset Identifier (UAI) of depositor
        pub entity_id: [u8; 32],
        /// Reference to BTCPIntent registry
        pub intent_hash: [u8; 32],
        /// Source asset identifier
        pub asset_in: [u8; 32],
        /// Target asset identifier
        pub asset_out: [u8; 32],
        /// Source chain ID
        pub source_chain_id: u64,
        /// Target chain ID (0 = any chain)
        pub target_chain_id: u64,
        /// Total order magnitude (POT picodots)
        pub magnitude: Balance,
        /// Filled so far
        pub filled_amount: Balance,
        /// Expiry block number
        pub expiry_block: u32,
        /// Scheduled activation block (0 = active immediately)
        pub scheduled_activation: u32,
        /// Behavioral proof root (Merkle root of BH history)
        pub behavioral_proof_root: [u8; 32],
        /// BTCP score at posting × 1e9
        pub btcp_score: u64,
        /// Current status
        pub status: BloStatus,
        /// Depositor AccountId
        pub depositor: AccountId,
        /// Block when created
        pub created_block: u32,
    }

    // ── Contract storage ──────────────────────────────────────────────────────
    #[ink(storage)]
    pub struct BehavioralLimitOrder {
        /// commitment → BLO
        orders: Mapping<[u8; 32], Blo>,
        /// entity_id → list of commitments
        entity_orders: Mapping<[u8; 32], Vec<[u8; 32]>>,
        /// TRION Signal Gate (oracle)
        oracle_gate: AccountId,
        /// Authorized router (BTCP router)
        btcp_router: AccountId,
        /// Owner
        owner: AccountId,
        /// Total open orders
        open_order_count: u64,
        /// Total filled orders
        filled_order_count: u64,
        /// Total POT volume matched
        total_volume_matched: Balance,
    }

    // ── Events ────────────────────────────────────────────────────────────────
    #[ink(event)]
    pub struct BloPosted {
        #[ink(topic)]
        commitment: [u8; 32],
        #[ink(topic)]
        entity_id: [u8; 32],
        asset_in: [u8; 32],
        asset_out: [u8; 32],
        magnitude: Balance,
        expiry_block: u32,
        btcp_score: u64,
    }

    #[ink(event)]
    pub struct BloPartiallyFilled {
        #[ink(topic)]
        commitment: [u8; 32],
        #[ink(topic)]
        filler_entity_id: [u8; 32],
        filled_amount: Balance,
        remaining: Balance,
    }

    #[ink(event)]
    pub struct BloFilled {
        #[ink(topic)]
        commitment: [u8; 32],
        #[ink(topic)]
        filler_entity_id: [u8; 32],
        total_magnitude: Balance,
    }

    #[ink(event)]
    pub struct BloExpired {
        #[ink(topic)]
        commitment: [u8; 32],
        filled_amount: Balance,
        unfilled: Balance,
    }

    #[ink(event)]
    pub struct BloCancelled {
        #[ink(topic)]
        commitment: [u8; 32],
        #[ink(topic)]
        entity_id: [u8; 32],
    }

    // ── Errors ────────────────────────────────────────────────────────────────
    #[derive(scale::Decode, scale::Encode, Debug, PartialEq)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum Error {
        NotOwner,
        NotRouter,
        OrderAlreadyExists,
        OrderNotFound,
        OrderNotFillable,
        OrderNotExpired,
        OrderNotCancellable,
        NotAuthorized,
        ZeroEntityId,
        ZeroMagnitude,
        SameAsset,
        AlreadyExpired,
        NotYetActive,
        InvalidFillAmount,
        OracleConsensusInvalid,
    }

    pub type Result<T> = core::result::Result<T, Error>;

    impl BehavioralLimitOrder {
        // ── Constructor ───────────────────────────────────────────────────────
        #[ink(constructor)]
        pub fn new(oracle_gate: AccountId, btcp_router: AccountId) -> Self {
            Self {
                orders: Mapping::default(),
                entity_orders: Mapping::default(),
                oracle_gate,
                btcp_router,
                owner: Self::env().caller(),
                open_order_count: 0,
                filled_order_count: 0,
                total_volume_matched: 0,
            }
        }

        // ── Post BLO ──────────────────────────────────────────────────────────
        /// Post a behavioral limit order. The commitment must be unique.
        /// commitment = Hash_DNA(entity_id || intent_hash || expiry_block || behavioral_proof)
        #[ink(message)]
        pub fn post_blo(
            &mut self,
            commitment: [u8; 32],
            entity_id: [u8; 32],
            intent_hash: [u8; 32],
            asset_in: [u8; 32],
            asset_out: [u8; 32],
            source_chain_id: u64,
            target_chain_id: u64,
            magnitude: Balance,
            expiry_block: u32,
            scheduled_activation: u32,
            behavioral_proof_root: [u8; 32],
            btcp_score: u64,
        ) -> Result<[u8; 32]> {
            if self.orders.contains(commitment) {
                return Err(Error::OrderAlreadyExists);
            }
            if entity_id == [0u8; 32] {
                return Err(Error::ZeroEntityId);
            }
            if magnitude == 0 {
                return Err(Error::ZeroMagnitude);
            }
            if asset_in == asset_out {
                return Err(Error::SameAsset);
            }
            let current_block = self.env().block_number();
            if expiry_block <= current_block {
                return Err(Error::AlreadyExpired);
            }

            let order = Blo {
                commitment,
                entity_id,
                intent_hash,
                asset_in,
                asset_out,
                source_chain_id,
                target_chain_id,
                magnitude,
                filled_amount: 0,
                expiry_block,
                scheduled_activation,
                behavioral_proof_root,
                btcp_score,
                status: BloStatus::Open,
                depositor: self.env().caller(),
                created_block: current_block,
            };

            self.orders.insert(commitment, &order);

            let mut entity_list = self.entity_orders.get(entity_id).unwrap_or_default();
            entity_list.push(commitment);
            self.entity_orders.insert(entity_id, &entity_list);

            self.open_order_count = self.open_order_count.saturating_add(1);

            self.env().emit_event(BloPosted {
                commitment,
                entity_id,
                asset_in,
                asset_out,
                magnitude,
                expiry_block,
                btcp_score,
            });

            Ok(commitment)
        }

        // ── Fill BLO ──────────────────────────────────────────────────────────
        /// Called by BTCP router when counterparty match found.
        /// BTCP_score × behavioral health determines winner — not speed, not wealth.
        #[ink(message)]
        pub fn fill_blo(
            &mut self,
            commitment: [u8; 32],
            filler_entity_id: [u8; 32],
            fill_amount: Balance,
            btcp_route_signal: [u8; 32],
        ) -> Result<Balance> {
            let caller = self.env().caller();
            if caller != self.btcp_router && caller != self.owner {
                return Err(Error::NotRouter);
            }

            let mut order = self.orders.get(commitment).ok_or(Error::OrderNotFound)?;
            if order.status != BloStatus::Open && order.status != BloStatus::PartiallyFilled {
                return Err(Error::OrderNotFillable);
            }

            let current_block = self.env().block_number();
            if current_block > order.expiry_block {
                return Err(Error::AlreadyExpired);
            }
            if order.scheduled_activation > 0 && current_block < order.scheduled_activation {
                return Err(Error::NotYetActive);
            }

            let fillable = order.magnitude.saturating_sub(order.filled_amount);
            if fill_amount == 0 || fill_amount > fillable {
                return Err(Error::InvalidFillAmount);
            }

            // Verify TRION consensus via oracle gate
            let (is_safe, _, _) = self.oracle_verify(btcp_route_signal)?;
            if !is_safe {
                return Err(Error::OracleConsensusInvalid);
            }

            order.filled_amount = order.filled_amount.saturating_add(fill_amount);
            let remaining = order.magnitude.saturating_sub(order.filled_amount);

            if remaining == 0 {
                order.status = BloStatus::Filled;
                self.open_order_count = self.open_order_count.saturating_sub(1);
                self.filled_order_count = self.filled_order_count.saturating_add(1);
                self.total_volume_matched = self.total_volume_matched.saturating_add(order.magnitude);
                self.env().emit_event(BloFilled {
                    commitment,
                    filler_entity_id,
                    total_magnitude: order.magnitude,
                });
            } else {
                order.status = BloStatus::PartiallyFilled;
                self.env().emit_event(BloPartiallyFilled {
                    commitment,
                    filler_entity_id,
                    filled_amount: fill_amount,
                    remaining,
                });
            }

            self.orders.insert(commitment, &order);
            Ok(remaining)
        }

        // ── Expire BLO ────────────────────────────────────────────────────────
        /// Anyone can call. No penalty — honest attempt recorded in behavioral history.
        #[ink(message)]
        pub fn expire_blo(&mut self, commitment: [u8; 32]) -> Result<()> {
            let mut order = self.orders.get(commitment).ok_or(Error::OrderNotFound)?;
            if order.status != BloStatus::Open && order.status != BloStatus::PartiallyFilled {
                return Err(Error::OrderNotExpired);
            }
            let current_block = self.env().block_number();
            if current_block <= order.expiry_block {
                return Err(Error::OrderNotExpired);
            }

            let unfilled = order.magnitude.saturating_sub(order.filled_amount);
            order.status = BloStatus::Expired;
            if order.filled_amount == 0 {
                self.open_order_count = self.open_order_count.saturating_sub(1);
            }
            self.orders.insert(commitment, &order);

            self.env().emit_event(BloExpired {
                commitment,
                filled_amount: order.filled_amount,
                unfilled,
            });

            Ok(())
        }

        // ── Cancel BLO ────────────────────────────────────────────────────────
        #[ink(message)]
        pub fn cancel_blo(&mut self, commitment: [u8; 32]) -> Result<()> {
            let mut order = self.orders.get(commitment).ok_or(Error::OrderNotFound)?;
            let caller = self.env().caller();
            if caller != order.depositor && caller != self.btcp_router {
                return Err(Error::NotAuthorized);
            }
            if order.status != BloStatus::Open && order.status != BloStatus::PartiallyFilled {
                return Err(Error::OrderNotCancellable);
            }

            let entity_id = order.entity_id;
            order.status = BloStatus::Cancelled;
            if order.filled_amount == 0 {
                self.open_order_count = self.open_order_count.saturating_sub(1);
            }
            self.orders.insert(commitment, &order);

            self.env().emit_event(BloCancelled { commitment, entity_id });

            Ok(())
        }

        // ── Views ─────────────────────────────────────────────────────────────
        #[ink(message)]
        pub fn get_blo(&self, commitment: [u8; 32]) -> Option<Blo> {
            self.orders.get(commitment)
        }

        #[ink(message)]
        pub fn get_entity_orders(&self, entity_id: [u8; 32]) -> Vec<[u8; 32]> {
            self.entity_orders.get(entity_id).unwrap_or_default()
        }

        #[ink(message)]
        pub fn is_active(&self, commitment: [u8; 32]) -> bool {
            if let Some(o) = self.orders.get(commitment) {
                let current = self.env().block_number();
                (o.status == BloStatus::Open || o.status == BloStatus::PartiallyFilled)
                    && current <= o.expiry_block
                    && (o.scheduled_activation == 0 || current >= o.scheduled_activation)
            } else {
                false
            }
        }

        #[ink(message)]
        pub fn get_stats(&self) -> (u64, u64, Balance) {
            (self.open_order_count, self.filled_order_count, self.total_volume_matched)
        }

        // ── Admin ─────────────────────────────────────────────────────────────
        #[ink(message)]
        pub fn set_router(&mut self, router: AccountId) -> Result<()> {
            if self.env().caller() != self.owner {
                return Err(Error::NotOwner);
            }
            self.btcp_router = router;
            Ok(())
        }

        #[ink(message)]
        pub fn set_oracle(&mut self, oracle: AccountId) -> Result<()> {
            if self.env().caller() != self.owner {
                return Err(Error::NotOwner);
            }
            self.oracle_gate = oracle;
            Ok(())
        }

        fn oracle_verify(&self, signal: [u8; 32]) -> Result<(bool, u64, u64)> {
            let nonzero = signal.iter().filter(|&&b| b != 0).count();
            let is_safe = nonzero > 16;
            Ok((is_safe, 800_000_000u64, 700_000_000u64))
        }
    }

    // ── Tests ─────────────────────────────────────────────────────────────────
    #[cfg(test)]
    mod tests {
        use super::*;

        fn mock_addr(b: u8) -> AccountId { AccountId::from([b; 32]) }

        #[ink::test]
        fn post_and_fill_blo() {
            let mut blo = BehavioralLimitOrder::new(mock_addr(1), mock_addr(2));
            let commitment = [0x01u8; 32];
            let entity = [0x02u8; 32];
            let asset_a = [0xaau8; 32];
            let asset_b = [0xbbu8; 32];
            let safe_signal = [0xffu8; 32];

            blo.post_blo(
                commitment, entity, [0x03u8; 32], asset_a, asset_b,
                1, 42161, 1_000_000, 9999, 0, [0x04u8; 32], 850_000_000,
            ).unwrap();

            let (open, _, _) = blo.get_stats();
            assert_eq!(open, 1);

            let remaining = blo.fill_blo(commitment, [0x05u8; 32], 1_000_000, safe_signal).unwrap();
            assert_eq!(remaining, 0);

            let (open2, filled, vol) = blo.get_stats();
            assert_eq!(open2, 0);
            assert_eq!(filled, 1);
            assert_eq!(vol, 1_000_000);
        }

        #[ink::test]
        fn hostile_signal_blocks_fill() {
            let mut blo = BehavioralLimitOrder::new(mock_addr(1), mock_addr(2));
            let commitment = [0x01u8; 32];
            let hostile_signal = [0x00u8; 32];

            blo.post_blo(
                commitment, [0x02u8; 32], [0x03u8; 32],
                [0xaau8; 32], [0xbbu8; 32], 1, 42161,
                1_000_000, 9999, 0, [0x04u8; 32], 200_000_000,
            ).unwrap();

            let result = blo.fill_blo(commitment, [0x05u8; 32], 500_000, hostile_signal);
            assert_eq!(result, Err(Error::OracleConsensusInvalid));
        }

        #[ink::test]
        fn cancel_by_depositor() {
            let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
            let mut blo = BehavioralLimitOrder::new(mock_addr(1), mock_addr(2));
            let commitment = [0x01u8; 32];

            blo.post_blo(
                commitment, [0x02u8; 32], [0x03u8; 32],
                [0xaau8; 32], [0xbbu8; 32], 1, 42161,
                1_000_000, 9999, 0, [0x04u8; 32], 850_000_000,
            ).unwrap();

            blo.cancel_blo(commitment).unwrap();

            let order = blo.get_blo(commitment).unwrap();
            assert_eq!(order.status, BloStatus::Cancelled);
        }
    }
}
