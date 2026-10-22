//! MeritPay Claim Contract
//!
//! Holds payroll escrow released by the payroll contract after a verified batch.
//! Employees claim individual payouts by presenting a Groth16 claim proof tied
//! to their nullifier from the payroll batch.

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, symbol_short, token, Address, Bytes, BytesN, Env, Vec,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ClaimError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    NullifierNotAuthorized = 3,
    AlreadyClaimed = 4,
    InvalidProof = 5,
    InvalidAmount = 6,
    InsufficientEscrow = 7,
    PayrollEpochNotExecuted = 8,
    SignalMismatch = 9,
    /// Caller is not authorized as admin.
    Unauthorized = 10,
    /// The contract is currently paused by the admin.
    ContractPaused = 11,
    /// No pending admin transfer exists to accept.
    NoPendingAdmin = 12,
    /// Timelock delay period has not elapsed yet.
    TimelockNotExpired = 13,
    /// No pending verifier rotation exists to execute or cancel.
    NoPendingVerifier = 14,
    /// No pending payroll rotation exists to execute or cancel.
    NoPendingPayroll = 15,
    /// New payroll contract epoch is lower than current epoch.
    EpochRegression = 16,
    /// Proposed new admin is identical to current admin.
    SameAdmin = 17,
}

/// Rotation delay in ledgers (~24 hours on Stellar Mainnet at ~5s per ledger).
pub const ROTATION_DELAY_LEDGERS: u32 = 17_280;

#[inline]
fn key_admin() -> soroban_sdk::Symbol {
    symbol_short!("admin")
}
#[inline]
fn key_pending_admin() -> soroban_sdk::Symbol {
    symbol_short!("p_admin")
}
#[inline]
fn key_payroll() -> soroban_sdk::Symbol {
    symbol_short!("payroll")
}
#[inline]
fn key_verifier() -> soroban_sdk::Symbol {
    symbol_short!("verifier")
}
#[inline]
fn key_token() -> soroban_sdk::Symbol {
    symbol_short!("token")
}
#[inline]
fn key_paused() -> soroban_sdk::Symbol {
    symbol_short!("paused")
}
#[inline]
fn key_last_paused_seq() -> soroban_sdk::Symbol {
    symbol_short!("lp_seq")
}
#[inline]
fn key_last_seen_epoch() -> soroban_sdk::Symbol {
    symbol_short!("ls_epoch")
}
#[inline]
fn key_pending_verifier() -> soroban_sdk::Symbol {
    symbol_short!("p_ver")
}
#[inline]
fn key_pending_verifier_seq() -> soroban_sdk::Symbol {
    symbol_short!("p_v_seq")
}
#[inline]
fn key_pending_verifier_prop_seq() -> soroban_sdk::Symbol {
    symbol_short!("pv_pseq")
}
#[inline]
fn key_pending_payroll() -> soroban_sdk::Symbol {
    symbol_short!("p_pay")
}
#[inline]
fn key_pending_payroll_seq() -> soroban_sdk::Symbol {
    symbol_short!("p_p_seq")
}
#[inline]
fn key_pending_payroll_prop_seq() -> soroban_sdk::Symbol {
    symbol_short!("pp_pseq")
}

mod verifier_contract {
    use soroban_sdk::{contractclient, Bytes, BytesN, Env, Vec};

    #[contractclient(name = "VerifierClient")]
    pub trait VerifierInterface {
        fn verify(env: Env, proof_bytes: Bytes, public_signals: Vec<BytesN<32>>) -> bool;
    }
}

mod payroll_contract {
    use soroban_sdk::{contractclient, BytesN, Env};

    #[contractclient(name = "PayrollClient")]
    pub trait PayrollInterface {
        fn is_nullifier_spent(env: Env, nullifier: BytesN<32>) -> bool;
        fn get_epoch(env: Env) -> u64;
    }
}

use payroll_contract::PayrollClient;
use verifier_contract::VerifierClient;

#[contract]
pub struct ClaimContract;

#[contractimpl]
impl ClaimContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        payroll_contract: Address,
        verifier_contract: Address,
        token_address: Address,
    ) -> Result<(), ClaimError> {
        if env.storage().persistent().has(&key_admin()) {
            return Err(ClaimError::AlreadyInitialized);
        }

        admin.require_auth();

        env.storage().persistent().set(&key_admin(), &admin);
        env.storage()
            .persistent()
            .set(&key_payroll(), &payroll_contract);
        env.storage()
            .persistent()
            .set(&key_verifier(), &verifier_contract);
        env.storage().persistent().set(&key_token(), &token_address);
        env.storage()
            .persistent()
            .set(&key_last_seen_epoch(), &0u64);

        env.events().publish(
            (symbol_short!("init"),),
            (admin, payroll_contract, verifier_contract, token_address),
        );

        Ok(())
    }

    /// Withdraw a verified payout to `recipient`.
    ///
    /// Public signals (claim circuit): [nullifier, payrollEpoch, amount]
    pub fn claim_payout(
        env: Env,
        recipient: Address,
        proof: Bytes,
        public_signals: Vec<BytesN<32>>,
        nullifier: BytesN<32>,
        amount: i128,
    ) -> Result<bool, ClaimError> {
        Self::assert_initialized(&env)?;

        if env
            .storage()
            .persistent()
            .get(&key_paused())
            .unwrap_or(false)
        {
            return Err(ClaimError::ContractPaused);
        }

        if amount <= 0 {
            return Err(ClaimError::InvalidAmount);
        }

        recipient.require_auth();

        if public_signals.len() != 3 {
            return Err(ClaimError::SignalMismatch);
        }

        let first_signal = public_signals.get(0).ok_or(ClaimError::SignalMismatch)?;
        if first_signal != nullifier {
            return Err(ClaimError::SignalMismatch);
        }

        let payroll_addr: Address = env
            .storage()
            .persistent()
            .get(&key_payroll())
            .ok_or(ClaimError::NotInitialized)?;
        let payroll = PayrollClient::new(&env, &payroll_addr);

        // Nullifier must have been spent in a payroll batch to be valid
        if !payroll.is_nullifier_spent(&nullifier) {
            return Err(ClaimError::NullifierNotAuthorized);
        }

        let claimed: bool = env
            .storage()
            .persistent()
            .get::<BytesN<32>, bool>(&nullifier)
            .unwrap_or(false);
        if claimed {
            return Err(ClaimError::AlreadyClaimed);
        }

        let stored_last_seen: u64 = env
            .storage()
            .persistent()
            .get(&key_last_seen_epoch())
            .unwrap_or(0);
        let on_chain_epoch = match payroll.try_get_epoch() {
            Ok(Ok(epoch)) => {
                if epoch > stored_last_seen {
                    env.storage().persistent().set(&key_last_seen_epoch(), &epoch);
                    epoch
                } else {
                    stored_last_seen
                }
            }
            _ => stored_last_seen,
        };
        let proof_epoch_bytes = public_signals.get(1).ok_or(ClaimError::SignalMismatch)?;
        let proof_epoch = Self::bytes_to_u64(&proof_epoch_bytes)?;
        if on_chain_epoch < proof_epoch {
            return Err(ClaimError::PayrollEpochNotExecuted);
        }

        let proof_amount_bytes = public_signals.get(2).ok_or(ClaimError::SignalMismatch)?;
        let proof_amount = Self::bytes_to_i128(&proof_amount_bytes)?;
        if proof_amount != amount {
            return Err(ClaimError::SignalMismatch);
        }

        let verifier_addr: Address = env
            .storage()
            .persistent()
            .get(&key_verifier())
            .ok_or(ClaimError::NotInitialized)?;
        let verifier = VerifierClient::new(&env, &verifier_addr);
        if !verifier.verify(&proof, &public_signals) {
            return Err(ClaimError::InvalidProof);
        }

        env.storage().persistent().set(&nullifier, &true);

        let token_addr: Address = env
            .storage()
            .persistent()
            .get(&key_token())
            .ok_or(ClaimError::NotInitialized)?;
        let token = token::Client::new(&env, &token_addr);
        token.transfer(&env.current_contract_address(), &recipient, &amount);

        env.events()
            .publish((symbol_short!("claim"),), (recipient, nullifier, amount));

        Ok(true)
    }

    pub fn is_claimed(env: Env, nullifier: BytesN<32>) -> bool {
        env.storage()
            .persistent()
            .get::<BytesN<32>, bool>(&nullifier)
            .unwrap_or(false)
    }

    // -----------------------------------------------------------------------
    // Verifier rotation with timelock
    //
    // Split into propose_verifier and execute_verifier with ROTATION_DELAY_LEDGERS
    // delay to prevent instant malicious verifier swap draining escrow.
    // -----------------------------------------------------------------------

    pub fn propose_verifier(
        env: Env,
        admin: Address,
        verifier_contract: Address,
    ) -> Result<(), ClaimError> {
        Self::assert_initialized(&env)?;

        if env.storage().persistent().get(&key_paused()).unwrap_or(false) {
            return Err(ClaimError::ContractPaused);
        }

        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&key_admin())
            .ok_or(ClaimError::NotInitialized)?;
        if admin != stored_admin {
            return Err(ClaimError::Unauthorized);
        }

        let now = env.ledger().sequence();
        let valid_at = now + ROTATION_DELAY_LEDGERS;
        env.storage()
            .persistent()
            .set(&key_pending_verifier(), &verifier_contract);
        env.storage()
            .persistent()
            .set(&key_pending_verifier_seq(), &valid_at);
        env.storage()
            .persistent()
            .set(&key_pending_verifier_prop_seq(), &now);

        env.events()
            .publish((symbol_short!("prop_ver"),), (verifier_contract, valid_at));

        Ok(())
    }

    pub fn execute_verifier(env: Env, admin: Address) -> Result<(), ClaimError> {
        Self::assert_initialized(&env)?;

        if env.storage().persistent().get(&key_paused()).unwrap_or(false) {
            return Err(ClaimError::ContractPaused);
        }

        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&key_admin())
            .ok_or(ClaimError::NotInitialized)?;
        if admin != stored_admin {
            return Err(ClaimError::Unauthorized);
        }

        let pending_verifier: Address = env
            .storage()
            .persistent()
            .get(&key_pending_verifier())
            .ok_or(ClaimError::NoPendingVerifier)?;
        let valid_at: u32 = env
            .storage()
            .persistent()
            .get(&key_pending_verifier_seq())
            .ok_or(ClaimError::NoPendingVerifier)?;

        if env.ledger().sequence() < valid_at {
            return Err(ClaimError::TimelockNotExpired);
        }

        let prop_seq: u32 = env
            .storage()
            .persistent()
            .get(&key_pending_verifier_prop_seq())
            .unwrap_or(0);
        if let Some(last_paused) = env
            .storage()
            .persistent()
            .get::<_, u32>(&key_last_paused_seq())
        {
            if last_paused >= prop_seq {
                return Err(ClaimError::ContractPaused);
            }
        }

        let old_verifier: Address = env
            .storage()
            .persistent()
            .get(&key_verifier())
            .ok_or(ClaimError::NotInitialized)?;
        env.storage()
            .persistent()
            .set(&key_verifier(), &pending_verifier);
        env.storage().persistent().remove(&key_pending_verifier());
        env.storage()
            .persistent()
            .remove(&key_pending_verifier_seq());
        env.storage()
            .persistent()
            .remove(&key_pending_verifier_prop_seq());

        env.events().publish(
            (symbol_short!("set_ver"),),
            (old_verifier, pending_verifier),
        );

        Ok(())
    }

    pub fn cancel_verifier_rotation(env: Env, admin: Address) -> Result<(), ClaimError> {
        Self::assert_initialized(&env)?;

        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&key_admin())
            .ok_or(ClaimError::NotInitialized)?;
        if admin != stored_admin {
            return Err(ClaimError::Unauthorized);
        }

        if !env.storage().persistent().has(&key_pending_verifier()) {
            return Err(ClaimError::NoPendingVerifier);
        }

        env.storage().persistent().remove(&key_pending_verifier());
        env.storage()
            .persistent()
            .remove(&key_pending_verifier_seq());
        env.storage()
            .persistent()
            .remove(&key_pending_verifier_prop_seq());

        env.events().publish((symbol_short!("canc_ver"),), admin);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Payroll contract rotation with timelock & epoch continuity check
    // -----------------------------------------------------------------------

    pub fn propose_payroll_contract(
        env: Env,
        admin: Address,
        payroll_contract: Address,
    ) -> Result<(), ClaimError> {
        Self::assert_initialized(&env)?;

        if env.storage().persistent().get(&key_paused()).unwrap_or(false) {
            return Err(ClaimError::ContractPaused);
        }

        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&key_admin())
            .ok_or(ClaimError::NotInitialized)?;
        if admin != stored_admin {
            return Err(ClaimError::Unauthorized);
        }

        let now = env.ledger().sequence();
        let valid_at = now + ROTATION_DELAY_LEDGERS;
        env.storage()
            .persistent()
            .set(&key_pending_payroll(), &payroll_contract);
        env.storage()
            .persistent()
            .set(&key_pending_payroll_seq(), &valid_at);
        env.storage()
            .persistent()
            .set(&key_pending_payroll_prop_seq(), &now);

        env.events()
            .publish((symbol_short!("prop_pay"),), (payroll_contract, valid_at));

        Ok(())
    }

    pub fn execute_payroll_contract(env: Env, admin: Address) -> Result<(), ClaimError> {
        Self::assert_initialized(&env)?;

        if env.storage().persistent().get(&key_paused()).unwrap_or(false) {
            return Err(ClaimError::ContractPaused);
        }

        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&key_admin())
            .ok_or(ClaimError::NotInitialized)?;
        if admin != stored_admin {
            return Err(ClaimError::Unauthorized);
        }

        let pending_payroll: Address = env
            .storage()
            .persistent()
            .get(&key_pending_payroll())
            .ok_or(ClaimError::NoPendingPayroll)?;
        let valid_at: u32 = env
            .storage()
            .persistent()
            .get(&key_pending_payroll_seq())
            .ok_or(ClaimError::NoPendingPayroll)?;

        if env.ledger().sequence() < valid_at {
            return Err(ClaimError::TimelockNotExpired);
        }

        let prop_seq: u32 = env
            .storage()
            .persistent()
            .get(&key_pending_payroll_prop_seq())
            .unwrap_or(0);
        if let Some(last_paused) = env
            .storage()
            .persistent()
            .get::<_, u32>(&key_last_paused_seq())
        {
            if last_paused >= prop_seq {
                return Err(ClaimError::ContractPaused);
            }
        }

        let current_payroll_addr: Address = env
            .storage()
            .persistent()
            .get(&key_payroll())
            .ok_or(ClaimError::NotInitialized)?;
        let current_payroll = PayrollClient::new(&env, &current_payroll_addr);

        let stored_last_seen: u64 = env
            .storage()
            .persistent()
            .get(&key_last_seen_epoch())
            .unwrap_or(0);

        // Safe epoch check: if current payroll contract is broken/trapping,
        // fall back to stored last-seen epoch so migration does not trap permanently.
        let current_epoch: u64 = match current_payroll.try_get_epoch() {
            Ok(Ok(epoch)) => {
                if epoch > stored_last_seen {
                    env.storage().persistent().set(&key_last_seen_epoch(), &epoch);
                    epoch
                } else {
                    stored_last_seen
                }
            }
            _ => stored_last_seen,
        };

        let new_payroll = PayrollClient::new(&env, &pending_payroll);
        let new_epoch = new_payroll.get_epoch();

        // Epoch continuity: new payroll contract cannot regress epoch
        if new_epoch < current_epoch {
            return Err(ClaimError::EpochRegression);
        }

        env.storage()
            .persistent()
            .set(&key_last_seen_epoch(), &new_epoch);

        env.storage()
            .persistent()
            .set(&key_payroll(), &pending_payroll);
        env.storage().persistent().remove(&key_pending_payroll());
        env.storage()
            .persistent()
            .remove(&key_pending_payroll_seq());
        env.storage()
            .persistent()
            .remove(&key_pending_payroll_prop_seq());

        env.events().publish(
            (symbol_short!("pay_set"),),
            (current_payroll_addr, pending_payroll),
        );

        Ok(())
    }

    pub fn cancel_payroll_rotation(env: Env, admin: Address) -> Result<(), ClaimError> {
        Self::assert_initialized(&env)?;

        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&key_admin())
            .ok_or(ClaimError::NotInitialized)?;
        if admin != stored_admin {
            return Err(ClaimError::Unauthorized);
        }

        if !env.storage().persistent().has(&key_pending_payroll()) {
            return Err(ClaimError::NoPendingPayroll);
        }

        env.storage().persistent().remove(&key_pending_payroll());
        env.storage()
            .persistent()
            .remove(&key_pending_payroll_seq());
        env.storage()
            .persistent()
            .remove(&key_pending_payroll_prop_seq());

        env.events().publish((symbol_short!("canc_pay"),), admin);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // set_paused
    //
    // Admin-only. Pauses or unpauses claim payouts during incidents or migration.
    // Pausing clears any pending verifier or payroll rotations to prevent
    // an admin from bypassing claimant exit windows.
    // -----------------------------------------------------------------------
    pub fn set_paused(env: Env, admin: Address, paused: bool) -> Result<(), ClaimError> {
        Self::assert_initialized(&env)?;

        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&key_admin())
            .ok_or(ClaimError::NotInitialized)?;
        if admin != stored_admin {
            return Err(ClaimError::Unauthorized);
        }

        env.storage().persistent().set(&key_paused(), &paused);

        if paused {
            env.storage()
                .persistent()
                .set(&key_last_paused_seq(), &env.ledger().sequence());

            // Pausing immediately clears any active pending rotation to prevent
            // an admin from bypassing claimant exit windows during a timelock delay.
            env.storage().persistent().remove(&key_pending_verifier());
            env.storage()
                .persistent()
                .remove(&key_pending_verifier_seq());
            env.storage()
                .persistent()
                .remove(&key_pending_verifier_prop_seq());

            env.storage().persistent().remove(&key_pending_payroll());
            env.storage()
                .persistent()
                .remove(&key_pending_payroll_seq());
            env.storage()
                .persistent()
                .remove(&key_pending_payroll_prop_seq());
        }

        env.events().publish((symbol_short!("pause"),), paused);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // transfer_admin
    //
    // Step 1 of two-step admin rotation. Admin sets `new_admin` as pending.
    // Rejects new_admin == admin to avoid no-op pending locks.
    // -----------------------------------------------------------------------
    pub fn transfer_admin(env: Env, admin: Address, new_admin: Address) -> Result<(), ClaimError> {
        Self::assert_initialized(&env)?;

        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&key_admin())
            .ok_or(ClaimError::NotInitialized)?;
        if admin != stored_admin {
            return Err(ClaimError::Unauthorized);
        }
        if new_admin == admin {
            return Err(ClaimError::SameAdmin);
        }

        env.storage()
            .persistent()
            .set(&key_pending_admin(), &new_admin);

        env.events()
            .publish((symbol_short!("adm_xfer"),), (admin, new_admin));

        Ok(())
    }

    // -----------------------------------------------------------------------
    // cancel_admin_transfer
    //
    // Admin-only. Cancels an active pending admin transfer.
    // -----------------------------------------------------------------------
    pub fn cancel_admin_transfer(env: Env, admin: Address) -> Result<(), ClaimError> {
        Self::assert_initialized(&env)?;

        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&key_admin())
            .ok_or(ClaimError::NotInitialized)?;
        if admin != stored_admin {
            return Err(ClaimError::Unauthorized);
        }

        if !env.storage().persistent().has(&key_pending_admin()) {
            return Err(ClaimError::NoPendingAdmin);
        }

        env.storage().persistent().remove(&key_pending_admin());

        env.events().publish((symbol_short!("adm_canc"),), admin);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // accept_admin
    //
    // Step 2 of two-step admin rotation. `new_admin` claims the admin role.
    // -----------------------------------------------------------------------
    pub fn accept_admin(env: Env, new_admin: Address) -> Result<(), ClaimError> {
        Self::assert_initialized(&env)?;

        new_admin.require_auth();
        let pending_admin: Address = env
            .storage()
            .persistent()
            .get(&key_pending_admin())
            .ok_or(ClaimError::NoPendingAdmin)?;
        if new_admin != pending_admin {
            return Err(ClaimError::Unauthorized);
        }

        let old_admin: Address = env
            .storage()
            .persistent()
            .get(&key_admin())
            .ok_or(ClaimError::NotInitialized)?;
        env.storage().persistent().set(&key_admin(), &new_admin);
        env.storage().persistent().remove(&key_pending_admin());

        env.events()
            .publish((symbol_short!("adm_acc"),), (old_admin, new_admin));

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Config getters
    // -----------------------------------------------------------------------

    pub fn get_admin(env: Env) -> Result<Address, ClaimError> {
        Self::assert_initialized(&env)?;
        env.storage()
            .persistent()
            .get(&key_admin())
            .ok_or(ClaimError::NotInitialized)
    }

    pub fn get_pending_admin(env: Env) -> Option<Address> {
        Self::assert_initialized(&env).ok()?;
        env.storage().persistent().get(&key_pending_admin())
    }

    pub fn get_payroll_contract(env: Env) -> Result<Address, ClaimError> {
        Self::assert_initialized(&env)?;
        env.storage()
            .persistent()
            .get(&key_payroll())
            .ok_or(ClaimError::NotInitialized)
    }

    pub fn get_verifier(env: Env) -> Result<Address, ClaimError> {
        Self::assert_initialized(&env)?;
        env.storage()
            .persistent()
            .get(&key_verifier())
            .ok_or(ClaimError::NotInitialized)
    }

    pub fn get_token(env: Env) -> Result<Address, ClaimError> {
        Self::assert_initialized(&env)?;
        env.storage()
            .persistent()
            .get(&key_token())
            .ok_or(ClaimError::NotInitialized)
    }

    pub fn is_paused(env: Env) -> bool {
        if Self::assert_initialized(&env).is_err() {
            return false;
        }
        env.storage()
            .persistent()
            .get(&key_paused())
            .unwrap_or(false)
    }

    pub fn get_pending_verifier(env: Env) -> Option<Address> {
        Self::assert_initialized(&env).ok()?;
        env.storage().persistent().get(&key_pending_verifier())
    }

    pub fn get_pending_verifier_valid_at(env: Env) -> Option<u32> {
        Self::assert_initialized(&env).ok()?;
        env.storage().persistent().get(&key_pending_verifier_seq())
    }

    pub fn get_pending_payroll_contract(env: Env) -> Option<Address> {
        Self::assert_initialized(&env).ok()?;
        env.storage().persistent().get(&key_pending_payroll())
    }

    pub fn get_pending_payroll_valid_at(env: Env) -> Option<u32> {
        Self::assert_initialized(&env).ok()?;
        env.storage().persistent().get(&key_pending_payroll_seq())
    }

    pub fn get_last_seen_epoch(env: Env) -> u64 {
        if Self::assert_initialized(&env).is_err() {
            return 0;
        }
        env.storage()
            .persistent()
            .get(&key_last_seen_epoch())
            .unwrap_or(0)
    }

    // -----------------------------------------------------------------------
    // Note on token immutability:
    // The SEP-41 token address (`token`) is intentionally immutable once set in
    // `initialize`. Escrow funds held by this contract are reserved for employee
    // claims. Permitting token address rotation would risk locked funds becoming
    // unwithdrawable or unauthorized asset substitution. If a different asset
    // is to be distributed, a new Claim contract instance must be deployed.
    // -----------------------------------------------------------------------

    /// Checks that the contract has been initialized.
    ///
    /// Note on sentinel choice:
    /// Using `key_admin()` as the initialization sentinel rather than `key_payroll()`
    /// is safe because `ClaimContract` does not possess an `upgrade` entrypoint.
    /// If an upgrade mechanism were ever introduced, legacy instances in persistent
    /// storage would lack `key_admin()`, potentially allowing re-initialization.
    /// Because this contract is immutable post-deployment (non-upgradeable),
    /// checking `key_admin()` is a fully sound initialization sentinel.
    fn assert_initialized(env: &Env) -> Result<(), ClaimError> {
        if !env.storage().persistent().has(&key_admin()) {
            return Err(ClaimError::NotInitialized);
        }
        Ok(())
    }

    /// Converts a 32-byte BN254 field-element public signal into a `u64`.
    ///
    /// The circuit does not constrain this signal to fit in 64 bits, so a
    /// malicious prover could choose a field element whose low 8 bytes equal
    /// any desired `u64` while the high 24 bytes are non-zero (i.e. the true
    /// field value is astronomically larger than the truncated result).
    /// Rejecting any non-zero high-order bytes ensures the on-chain integer
    /// is a faithful, non-wrapped representation of the signal.
    fn bytes_to_u64(bytes: &BytesN<32>) -> Result<u64, ClaimError> {
        let arr = bytes.to_array();
        if arr[0..24].iter().any(|&b| b != 0) {
            return Err(ClaimError::SignalMismatch);
        }
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&arr[24..32]);
        Ok(u64::from_be_bytes(buf))
    }

    /// Converts a 32-byte BN254 field-element public signal into an `i128`.
    /// See `bytes_to_u64` for why the high-order bytes must be checked.
    fn bytes_to_i128(bytes: &BytesN<32>) -> Result<i128, ClaimError> {
        let arr = bytes.to_array();
        if arr[0..16].iter().any(|&b| b != 0) {
            return Err(ClaimError::SignalMismatch);
        }
        let mut buf = [0u8; 16];
        buf.copy_from_slice(&arr[16..32]);
        Ok(i128::from_be_bytes(buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger as _},
        Env,
    };

    mod mock_ok {
        use soroban_sdk::{contract, contractimpl, Bytes, BytesN, Env, Vec};

        #[contract]
        pub struct MockOk;

        #[contractimpl]
        impl MockOk {
            pub fn verify(_env: Env, _proof: Bytes, _signals: Vec<BytesN<32>>) -> bool {
                true
            }
        }
    }

    mod mock_payroll {
        use soroban_sdk::{contract, contractimpl, BytesN, Env};

        #[contract]
        pub struct MockPayroll;

        #[contractimpl]
        impl MockPayroll {
            pub fn is_nullifier_spent(_env: Env, _nullifier: BytesN<32>) -> bool {
                true
            }

            pub fn get_epoch(_env: Env) -> u64 {
                2
            }
        }
    }

    fn setup(env: &Env) -> (Address, Address, Address, Address, Address, Address) {
        let admin = Address::generate(env);
        let recipient = Address::generate(env);
        let payroll_id = env.register(mock_payroll::MockPayroll, ());
        let verifier_id = env.register(mock_ok::MockOk, ());
        let xlm_id = env
            .register_stellar_asset_contract_v2(recipient.clone())
            .address();
        let claim_id = env.register(ClaimContract, ());

        env.mock_all_auths();

        let client = ClaimContractClient::new(env, &claim_id);
        client.initialize(&admin, &payroll_id, &verifier_id, &xlm_id);

        let xlm = token::StellarAssetClient::new(env, &xlm_id);
        xlm.mint(&recipient, &1_000_0000000i128);
        xlm.transfer(&recipient, &claim_id, &500_0000000i128);

        (admin, recipient, claim_id, xlm_id, payroll_id, verifier_id)
    }

    #[test]
    fn test_claim_payout_happy_path() {
        let env = Env::default();
        let (_admin, recipient, claim_id, _xlm_id, _payroll_id, _verifier_id) = setup(&env);
        let client = ClaimContractClient::new(&env, &claim_id);

        let nullifier = BytesN::from_array(&env, &[0xABu8; 32]);
        let mut signals: Vec<BytesN<32>> = Vec::new(&env);
        signals.push_back(nullifier.clone());
        signals.push_back(BytesN::from_array(&env, &{
            let mut b = [0u8; 32];
            b[31] = 1;
            b
        }));
        signals.push_back(BytesN::from_array(&env, &{
            let mut b = [0u8; 32];
            let amt: i128 = 10_0000000;
            b[16..32].copy_from_slice(&amt.to_be_bytes());
            b
        }));

        let proof = Bytes::from_slice(&env, &[0u8; 256]);
        let result = client.claim_payout(&recipient, &proof, &signals, &nullifier, &10_0000000i128);
        assert!(result);
        assert!(client.is_claimed(&nullifier));
    }

    #[test]
    fn test_claim_config_getters() {
        let env = Env::default();
        let (admin, _recipient, claim_id, xlm_id, payroll_id, verifier_id) = setup(&env);
        let client = ClaimContractClient::new(&env, &claim_id);

        assert_eq!(client.get_admin(), admin);
        assert_eq!(client.get_payroll_contract(), payroll_id);
        assert_eq!(client.get_verifier(), verifier_id);
        assert_eq!(client.get_token(), xlm_id);
        assert_eq!(client.get_pending_admin(), None);
        assert_eq!(client.get_pending_verifier(), None);
        assert_eq!(client.get_pending_payroll_contract(), None);
        assert_eq!(client.is_paused(), false);
    }

    #[test]
    fn test_claim_timelocked_verifier_rotation_lifecycle() {
        let env = Env::default();
        let (admin, _recipient, claim_id, _xlm_id, _payroll_id, initial_verifier) = setup(&env);
        let client = ClaimContractClient::new(&env, &claim_id);

        let new_verifier = env.register(mock_ok::MockOk, ());
        let attacker = Address::generate(&env);

        // Attacker cannot propose rotation
        let fail = client.try_propose_verifier(&attacker, &new_verifier);
        assert!(fail.is_err());

        // Admin proposes verifier rotation
        client.propose_verifier(&admin, &new_verifier);
        assert_eq!(client.get_pending_verifier(), Some(new_verifier.clone()));
        assert_eq!(client.get_verifier(), initial_verifier);

        // Executing before timelock elapses fails
        let early = client.try_execute_verifier(&admin);
        assert_eq!(early, Err(Ok(ClaimError::TimelockNotExpired)));

        // Advance ledger sequence past delay
        let cur_seq = env.ledger().sequence();
        env.ledger()
            .set_sequence_number(cur_seq + ROTATION_DELAY_LEDGERS);

        // Attacker cannot execute
        let unauth_exec = client.try_execute_verifier(&attacker);
        assert!(unauth_exec.is_err());

        // Admin executes after delay
        client.execute_verifier(&admin);
        assert_eq!(client.get_verifier(), new_verifier);
        assert_eq!(client.get_pending_verifier(), None);

        // Cancellation test
        let another_verifier = env.register(mock_ok::MockOk, ());
        client.propose_verifier(&admin, &another_verifier);
        assert_eq!(
            client.get_pending_verifier(),
            Some(another_verifier.clone())
        );
        client.cancel_verifier_rotation(&admin);
        assert_eq!(client.get_pending_verifier(), None);
    }

    #[test]
    fn test_claim_timelocked_payroll_rotation_lifecycle() {
        let env = Env::default();
        let (admin, _recipient, claim_id, _xlm_id, initial_payroll, _verifier_id) = setup(&env);
        let client = ClaimContractClient::new(&env, &claim_id);
        assert_eq!(client.get_payroll_contract(), initial_payroll);

        let new_payroll = env.register(mock_payroll::MockPayroll, ());
        let attacker = Address::generate(&env);

        // Attacker cannot propose
        let fail = client.try_propose_payroll_contract(&attacker, &new_payroll);
        assert!(fail.is_err());

        // Admin proposes payroll rotation
        client.propose_payroll_contract(&admin, &new_payroll);
        assert_eq!(
            client.get_pending_payroll_contract(),
            Some(new_payroll.clone())
        );

        // Early execution fails
        let early = client.try_execute_payroll_contract(&admin);
        assert_eq!(early, Err(Ok(ClaimError::TimelockNotExpired)));

        // Advance ledger
        let cur_seq = env.ledger().sequence();
        env.ledger()
            .set_sequence_number(cur_seq + ROTATION_DELAY_LEDGERS);

        // Execution succeeds
        client.execute_payroll_contract(&admin);
        assert_eq!(client.get_payroll_contract(), new_payroll);
        assert_eq!(client.get_pending_payroll_contract(), None);

        // Cancellation test
        let third_payroll = env.register(mock_payroll::MockPayroll, ());
        client.propose_payroll_contract(&admin, &third_payroll);
        assert_eq!(
            client.get_pending_payroll_contract(),
            Some(third_payroll.clone())
        );
        client.cancel_payroll_rotation(&admin);
        assert_eq!(client.get_pending_payroll_contract(), None);
    }

    #[test]
    fn test_claim_set_paused_halts_and_resumes() {
        let env = Env::default();
        let (admin, recipient, claim_id, _xlm_id, _payroll_id, _verifier_id) = setup(&env);
        let client = ClaimContractClient::new(&env, &claim_id);

        let attacker = Address::generate(&env);

        // Attacker cannot pause
        let unauth = client.try_set_paused(&attacker, &true);
        assert!(unauth.is_err());

        // Admin pauses
        client.set_paused(&admin, &true);
        assert!(client.is_paused());

        let nullifier = BytesN::from_array(&env, &[0xCDu8; 32]);
        let mut signals: Vec<BytesN<32>> = Vec::new(&env);
        signals.push_back(nullifier.clone());
        signals.push_back(BytesN::from_array(&env, &{
            let mut b = [0u8; 32];
            b[31] = 1;
            b
        }));
        signals.push_back(BytesN::from_array(&env, &{
            let mut b = [0u8; 32];
            let amt: i128 = 5_0000000;
            b[16..32].copy_from_slice(&amt.to_be_bytes());
            b
        }));
        let proof = Bytes::from_slice(&env, &[0u8; 256]);

        // Payout rejected when paused
        let paused_err =
            client.try_claim_payout(&recipient, &proof, &signals, &nullifier, &5_0000000i128);
        assert_eq!(paused_err, Err(Ok(ClaimError::ContractPaused)));

        // Unpause
        client.set_paused(&admin, &false);
        assert!(!client.is_paused());

        // Payout succeeds
        let res = client.claim_payout(&recipient, &proof, &signals, &nullifier, &5_0000000i128);
        assert!(res);
    }

    #[test]
    fn test_claim_two_step_admin_transfer_full_lifecycle() {
        let env = Env::default();
        let (admin, _recipient, claim_id, _xlm_id, _payroll_id, _verifier_id) = setup(&env);
        let client = ClaimContractClient::new(&env, &claim_id);

        let new_admin = Address::generate(&env);
        let attacker = Address::generate(&env);

        // Transfer to self rejected
        let same_admin = client.try_transfer_admin(&admin, &admin);
        assert_eq!(same_admin, Err(Ok(ClaimError::SameAdmin)));

        // Attacker cannot initiate transfer
        let unauth_xfer = client.try_transfer_admin(&attacker, &new_admin);
        assert!(unauth_xfer.is_err());

        // Accept with no pending transfer fails
        let no_pend = client.try_accept_admin(&new_admin);
        assert_eq!(no_pend, Err(Ok(ClaimError::NoPendingAdmin)));

        // Admin initiates transfer
        client.transfer_admin(&admin, &new_admin);
        assert_eq!(client.get_pending_admin(), Some(new_admin.clone()));
        assert_eq!(client.get_admin(), admin);

        // Cancel transfer
        client.cancel_admin_transfer(&admin);
        assert_eq!(client.get_pending_admin(), None);

        // Re-initiate transfer
        client.transfer_admin(&admin, &new_admin);

        // Attacker cannot accept
        let attacker_accept = client.try_accept_admin(&attacker);
        assert!(attacker_accept.is_err());

        // New admin accepts
        client.accept_admin(&new_admin);
        assert_eq!(client.get_admin(), new_admin);
        assert_eq!(client.get_pending_admin(), None);

        // Old admin loses privileges
        let old_admin_action = client.try_set_paused(&admin, &true);
        assert!(old_admin_action.is_err());

        // New admin has privileges
        client.set_paused(&new_admin, &true);
        assert!(client.is_paused());
    }

    #[test]
    fn test_claim_admin_auth_verification() {
        let env = Env::default();
        let (admin, _recipient, claim_id, _xlm_id, _payroll_id, _verifier_id) = setup(&env);
        let client = ClaimContractClient::new(&env, &claim_id);

        let candidate = Address::generate(&env);
        let new_ver = env.register(mock_ok::MockOk, ());
        let new_pay = env.register(mock_payroll::MockPayroll, ());

        // 1. set_paused
        client.set_paused(&admin, &false);
        assert_eq!(env.auths()[0].0, admin);

        // 2. transfer_admin
        client.transfer_admin(&admin, &candidate);
        assert_eq!(env.auths()[0].0, admin);

        // 3. cancel_admin_transfer
        client.cancel_admin_transfer(&admin);
        assert_eq!(env.auths()[0].0, admin);

        // 4. propose_verifier
        client.propose_verifier(&admin, &new_ver);
        assert_eq!(env.auths()[0].0, admin);

        // 5. cancel_verifier_rotation
        client.cancel_verifier_rotation(&admin);
        assert_eq!(env.auths()[0].0, admin);

        // 6. propose_verifier & execute_verifier
        client.propose_verifier(&admin, &new_ver);
        env.ledger()
            .set_sequence_number(env.ledger().sequence() + ROTATION_DELAY_LEDGERS);
        client.execute_verifier(&admin);
        assert_eq!(env.auths()[0].0, admin);

        // 7. propose_payroll_contract
        client.propose_payroll_contract(&admin, &new_pay);
        assert_eq!(env.auths()[0].0, admin);

        // 8. cancel_payroll_rotation
        client.cancel_payroll_rotation(&admin);
        assert_eq!(env.auths()[0].0, admin);

        // 9. propose_payroll_contract & execute_payroll_contract
        client.propose_payroll_contract(&admin, &new_pay);
        env.ledger()
            .set_sequence_number(env.ledger().sequence() + ROTATION_DELAY_LEDGERS);
        client.execute_payroll_contract(&admin);
        assert_eq!(env.auths()[0].0, admin);
    }

    #[test]
    fn test_claim_timelock_pause_bypass_prevention() {
        let env = Env::default();
        let (admin, _recipient, claim_id, _xlm_id, _payroll_id, _verifier_id) = setup(&env);
        let client = ClaimContractClient::new(&env, &claim_id);

        let new_verifier = env.register(mock_ok::MockOk, ());
        let new_payroll = env.register(mock_payroll::MockPayroll, ());

        // Admin pauses contract
        client.set_paused(&admin, &true);
        assert!(client.is_paused());

        // Cannot propose while paused
        assert_eq!(
            client.try_propose_verifier(&admin, &new_verifier),
            Err(Ok(ClaimError::ContractPaused))
        );
        assert_eq!(
            client.try_propose_payroll_contract(&admin, &new_payroll),
            Err(Ok(ClaimError::ContractPaused))
        );

        // Unpause
        client.set_paused(&admin, &false);
        assert!(!client.is_paused());

        // Propose verifier rotation
        client.propose_verifier(&admin, &new_verifier);
        assert_eq!(client.get_pending_verifier(), Some(new_verifier.clone()));

        // Pausing immediately wipes active pending rotation to prevent bypassing timelock exit window
        client.set_paused(&admin, &true);
        assert_eq!(client.get_pending_verifier(), None);

        // Advancing ledger and attempting execute fails because rotation was cleared on pause
        env.ledger()
            .set_sequence_number(env.ledger().sequence() + ROTATION_DELAY_LEDGERS);
        assert_eq!(
            client.try_execute_verifier(&admin),
            Err(Ok(ClaimError::ContractPaused))
        );

        // Unpause and verify pending is still None
        client.set_paused(&admin, &false);
        assert_eq!(
            client.try_execute_verifier(&admin),
            Err(Ok(ClaimError::NoPendingVerifier))
        );

        // Same verification for payroll rotation
        client.propose_payroll_contract(&admin, &new_payroll);
        assert_eq!(
            client.get_pending_payroll_contract(),
            Some(new_payroll.clone())
        );

        client.set_paused(&admin, &true);
        assert_eq!(client.get_pending_payroll_contract(), None);

        env.ledger()
            .set_sequence_number(env.ledger().sequence() + ROTATION_DELAY_LEDGERS);
        client.set_paused(&admin, &false);
        assert_eq!(
            client.try_execute_payroll_contract(&admin),
            Err(Ok(ClaimError::NoPendingPayroll))
        );
    }

    mod mock_broken_payroll {
        use soroban_sdk::{contract, contractimpl, BytesN, Env};

        #[contract]
        pub struct MockBrokenPayroll;

        #[contractimpl]
        impl MockBrokenPayroll {
            pub fn is_nullifier_spent(_env: Env, _nullifier: BytesN<32>) -> bool {
                true
            }

            pub fn get_epoch(_env: Env) -> u64 {
                panic!("Old payroll contract is broken/trapped");
            }
        }
    }

    #[test]
    fn test_claim_payroll_migration_broken_contract_fallback() {
        let env = Env::default();
        let (admin, _recipient, _claim_id, xlm_id, _payroll_id, verifier_id) = setup(&env);

        // Deploy broken payroll contract and set it as active
        let broken_payroll = env.register(mock_broken_payroll::MockBrokenPayroll, ());
        let third_payroll = env.register(mock_payroll::MockPayroll, ());

        // Re-initialize a fresh claim instance with broken payroll to test fallback
        let fresh_claim = env.register(ClaimContract, ());
        let fresh_client = ClaimContractClient::new(&env, &fresh_claim);
        fresh_client.initialize(&admin, &broken_payroll, &verifier_id, &xlm_id);

        // Propose migration to third payroll
        fresh_client.propose_payroll_contract(&admin, &third_payroll);
        env.ledger()
            .set_sequence_number(env.ledger().sequence() + ROTATION_DELAY_LEDGERS);

        // Migration succeeds despite broken old contract trapping on get_epoch()
        fresh_client.execute_payroll_contract(&admin);
        assert_eq!(fresh_client.get_payroll_contract(), third_payroll);
        assert_eq!(fresh_client.get_last_seen_epoch(), 2);
    }
}
