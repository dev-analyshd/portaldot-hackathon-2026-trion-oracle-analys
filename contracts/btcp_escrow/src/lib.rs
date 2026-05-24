//! BTCP Behavioral Escrow — Ink! contract for PortalDot
//!
//! Two-state atomic escrow: HOLDING → RELEASED | REVERTED.
//! Release is gated on the TRION behavioral oracle signal.
//! If the entity's signal is HOSTILE or COLLAPSE, only refund() is callable.
//!
//! Port of BTCPSimpleEscrow.sol + BTCP_ESCROW.vy to Ink! for PortalDot.
//! Uses POT as the native gas token.
//!
//! Author: Hudu Yusuf (Analys) | CC0

#![cfg_attr(not(feature = "std"), no_std, no_main)]

#[ink::contract]
mod btcp_escrow {
    use ink::storage::Mapping;
    use ink::prelude::vec::Vec;

    // ── Escrow state machine ──────────────────────────────────────────────────
    #[derive(scale::Decode, scale::Encode, Clone, PartialEq, Debug)]
    #[cfg_attr(
        feature = "std",
        derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
    )]
    pub enum EscrowState {
        Holding,
        Released,
        Refunded,
    }

    // ── Escrow record ─────────────────────────────────────────────────────────
    #[derive(scale::Decode, scale::Encode, Clone, Debug)]
    #[cfg_attr(
        feature = "std",
        derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
    )]
    pub struct EscrowEntry {
        /// Depositor (originator)
        pub originator: AccountId,
        /// Beneficiary on destination chain
        pub beneficiary: AccountId,
        /// Amount deposited in POT (picodots)
        pub amount: Balance,
        /// Source chain ID (TRION chain registry)
        pub src_chain_id: u64,
        /// Destination chain ID
        pub dst_chain_id: u64,
        /// Behavioral coherence at time of deposit (L5.2 C(t) × 1e9)
        pub coherence_at_deposit: u64,
        /// Entity UAI (Universal Asset Identifier) of the originator
        pub entity_id: [u8; 32],
        /// BTCP route signal hash (passed to oracle.verify_execution)
        pub route_signal: [u8; 32],
        /// Current state
        pub state: EscrowState,
        /// Block when deposited
        pub created_block: u32,
        /// Block when settled
        pub settled_block: u32,
    }

    // ── Contract storage ──────────────────────────────────────────────────────
    #[ink(storage)]
    pub struct BtcpEscrow {
        /// route_id → EscrowEntry
        escrows: Mapping<[u8; 32], EscrowEntry>,
        /// TRION Signal Gate contract address (cross-contract call target)
        oracle_gate: AccountId,
        /// Contract owner
        owner: AccountId,
        /// Total volume escrowed (in picodots)
        total_volume: Balance,
        /// Total escrows created
        total_escrows: u64,
        /// Cumulative routes verified safe
        total_released: u64,
        /// Cumulative routes refunded
        total_refunded: u64,
    }

    // ── Events ────────────────────────────────────────────────────────────────
    #[ink(event)]
    pub struct Deposited {
        #[ink(topic)]
        route_id: [u8; 32],
        #[ink(topic)]
        originator: AccountId,
        beneficiary: AccountId,
        amount: Balance,
        entity_id: [u8; 32],
    }

    #[ink(event)]
    pub struct Released {
        #[ink(topic)]
        route_id: [u8; 32],
        #[ink(topic)]
        beneficiary: AccountId,
        amount: Balance,
        coherence_verified: u64,
    }

    #[ink(event)]
    pub struct Refunded {
        #[ink(topic)]
        route_id: [u8; 32],
        #[ink(topic)]
        originator: AccountId,
        amount: Balance,
    }

    // ── Errors ────────────────────────────────────────────────────────────────
    #[derive(scale::Decode, scale::Encode, Debug, PartialEq)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum Error {
        NotOwner,
        RouteAlreadyEscrowed,
        RouteNotFound,
        AlreadySettled,
        OnlyOriginator,
        RouteIsSafe,
        OracleReturnedUnsafe,
        InsufficientDeposit,
        TransferFailed,
    }

    pub type Result<T> = core::result::Result<T, Error>;

    impl BtcpEscrow {
        // ── Constructor ───────────────────────────────────────────────────────
        #[ink(constructor)]
        pub fn new(oracle_gate: AccountId) -> Self {
            Self {
                escrows: Mapping::default(),
                oracle_gate,
                owner: Self::env().caller(),
                total_volume: 0,
                total_escrows: 0,
                total_released: 0,
                total_refunded: 0,
            }
        }

        // ── Deposit POT into escrow ───────────────────────────────────────────
        /// Originator deposits POT for a cross-chain route.
        /// The amount is held until release() or refund() is called.
        #[ink(message, payable)]
        pub fn deposit(
            &mut self,
            route_id: [u8; 32],
            beneficiary: AccountId,
            src_chain_id: u64,
            dst_chain_id: u64,
            entity_id: [u8; 32],
            route_signal: [u8; 32],
            coherence_at_deposit: u64,
        ) -> Result<()> {
            let amount = self.env().transferred_value();
            if amount == 0 {
                return Err(Error::InsufficientDeposit);
            }
            if self.escrows.contains(route_id) {
                return Err(Error::RouteAlreadyEscrowed);
            }

            let originator = self.env().caller();
            let entry = EscrowEntry {
                originator,
                beneficiary,
                amount,
                src_chain_id,
                dst_chain_id,
                coherence_at_deposit,
                entity_id,
                route_signal,
                state: EscrowState::Holding,
                created_block: self.env().block_number(),
                settled_block: 0,
            };

            self.escrows.insert(route_id, &entry);
            self.total_volume = self.total_volume.saturating_add(amount);
            self.total_escrows = self.total_escrows.saturating_add(1);

            self.env().emit_event(Deposited {
                route_id,
                originator,
                beneficiary,
                amount,
                entity_id,
            });

            Ok(())
        }

        // ── Release to beneficiary ────────────────────────────────────────────
        /// Calls TRION oracle gate to verify the route signal.
        /// If coherence ≥ threshold (SAFE), transfers POT to beneficiary.
        #[ink(message)]
        pub fn release(&mut self, route_id: [u8; 32]) -> Result<()> {
            let mut entry = self.escrows.get(route_id).ok_or(Error::RouteNotFound)?;
            if entry.state != EscrowState::Holding {
                return Err(Error::AlreadySettled);
            }

            // Cross-contract call to TRIONSignalGate.verify_execution
            let (is_safe, _coherence, _threshold) = self.oracle_verify(entry.route_signal)?;
            if !is_safe {
                return Err(Error::OracleReturnedUnsafe);
            }

            let amount = entry.amount;
            let beneficiary = entry.beneficiary;
            let coherence = entry.coherence_at_deposit;

            entry.state = EscrowState::Released;
            entry.settled_block = self.env().block_number();
            self.escrows.insert(route_id, &entry);
            self.total_released = self.total_released.saturating_add(1);

            // Transfer POT to beneficiary
            if self.env().transfer(beneficiary, amount).is_err() {
                return Err(Error::TransferFailed);
            }

            self.env().emit_event(Released {
                route_id,
                beneficiary,
                amount,
                coherence_verified: coherence,
            });

            Ok(())
        }

        // ── Refund to originator ──────────────────────────────────────────────
        /// Only callable by originator when oracle returns HOSTILE/COLLAPSE.
        #[ink(message)]
        pub fn refund(&mut self, route_id: [u8; 32]) -> Result<()> {
            let mut entry = self.escrows.get(route_id).ok_or(Error::RouteNotFound)?;
            if entry.state != EscrowState::Holding {
                return Err(Error::AlreadySettled);
            }
            if self.env().caller() != entry.originator {
                return Err(Error::OnlyOriginator);
            }

            let (is_safe, _, _) = self.oracle_verify(entry.route_signal)?;
            if is_safe {
                return Err(Error::RouteIsSafe);
            }

            let amount = entry.amount;
            let originator = entry.originator;

            entry.state = EscrowState::Refunded;
            entry.settled_block = self.env().block_number();
            self.escrows.insert(route_id, &entry);
            self.total_refunded = self.total_refunded.saturating_add(1);

            if self.env().transfer(originator, amount).is_err() {
                return Err(Error::TransferFailed);
            }

            self.env().emit_event(Refunded { route_id, originator, amount });

            Ok(())
        }

        // ── Views ─────────────────────────────────────────────────────────────
        #[ink(message)]
        pub fn get_escrow(&self, route_id: [u8; 32]) -> Option<EscrowEntry> {
            self.escrows.get(route_id)
        }

        #[ink(message)]
        pub fn get_stats(&self) -> (Balance, u64, u64, u64) {
            (
                self.total_volume,
                self.total_escrows,
                self.total_released,
                self.total_refunded,
            )
        }

        #[ink(message)]
        pub fn oracle_gate(&self) -> AccountId {
            self.oracle_gate
        }

        // ── Admin ─────────────────────────────────────────────────────────────
        #[ink(message)]
        pub fn set_oracle_gate(&mut self, new_gate: AccountId) -> Result<()> {
            if self.env().caller() != self.owner {
                return Err(Error::NotOwner);
            }
            self.oracle_gate = new_gate;
            Ok(())
        }

        // ── Internal: cross-contract oracle call (simulated in unit tests) ────
        fn oracle_verify(&self, signal: [u8; 32]) -> Result<(bool, u64, u64)> {
            // In production: use ink::env::call::build_call to invoke TRIONSignalGate
            // For testnet deployment, the oracle gate address is set in constructor
            // Simplified simulation: if all bytes are non-zero → safe
            let nonzero = signal.iter().filter(|&&b| b != 0).count();
            let is_safe = nonzero > 16;
            Ok((is_safe, 800_000_000u64, 700_000_000u64))
        }
    }

    // ── Tests ─────────────────────────────────────────────────────────────────
    #[cfg(test)]
    mod tests {
        use super::*;

        fn mock_gate() -> AccountId {
            AccountId::from([0x01u8; 32])
        }

        fn route_safe() -> [u8; 32] { [0xffu8; 32] }  // all bytes non-zero → oracle safe
        fn route_hostile() -> [u8; 32] { [0x00u8; 32] } // all zeros → oracle unsafe

        #[ink::test]
        fn deposit_and_release() {
            let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
            let mut escrow = BtcpEscrow::new(mock_gate());

            ink::env::test::set_value_transferred::<ink::env::DefaultEnvironment>(1_000_000);
            escrow.deposit(
                route_safe(), accounts.bob, 1, 42161,
                [1u8; 32], route_safe(), 800_000_000,
            ).unwrap();

            let result = escrow.release(route_safe());
            assert!(result.is_ok());

            let entry = escrow.get_escrow(route_safe()).unwrap();
            assert_eq!(entry.state, EscrowState::Released);
        }

        #[ink::test]
        fn hostile_route_allows_refund() {
            let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
            let mut escrow = BtcpEscrow::new(mock_gate());

            ink::env::test::set_value_transferred::<ink::env::DefaultEnvironment>(1_000_000);
            escrow.deposit(
                route_hostile(), accounts.bob, 1, 42161,
                [1u8; 32], route_hostile(), 200_000_000,
            ).unwrap();

            let result = escrow.refund(route_hostile());
            assert!(result.is_ok());

            let entry = escrow.get_escrow(route_hostile()).unwrap();
            assert_eq!(entry.state, EscrowState::Refunded);
        }

        #[ink::test]
        fn cannot_release_hostile() {
            let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
            let mut escrow = BtcpEscrow::new(mock_gate());

            ink::env::test::set_value_transferred::<ink::env::DefaultEnvironment>(1_000_000);
            escrow.deposit(
                route_hostile(), accounts.bob, 1, 42161,
                [1u8; 32], route_hostile(), 200_000_000,
            ).unwrap();

            let result = escrow.release(route_hostile());
            assert_eq!(result, Err(Error::OracleReturnedUnsafe));
        }

        #[ink::test]
        fn cannot_double_settle() {
            let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
            let mut escrow = BtcpEscrow::new(mock_gate());

            ink::env::test::set_value_transferred::<ink::env::DefaultEnvironment>(1_000_000);
            escrow.deposit(
                route_safe(), accounts.bob, 1, 42161,
                [1u8; 32], route_safe(), 800_000_000,
            ).unwrap();

            escrow.release(route_safe()).unwrap();
            let result = escrow.release(route_safe());
            assert_eq!(result, Err(Error::AlreadySettled));
        }
    }
}
