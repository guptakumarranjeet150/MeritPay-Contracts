//! MeritPay Payroll Contract
//!
//! Holds an XLM (or USDC) pool, verifies Groth16 payroll proofs via the
//! Groth16Verifier contract, tracks spent nullifiers to prevent double-spending,
//! records pool debits upon successful verification, and transfers escrow to the
//! linked claim contract for employee withdrawals.
//!
//! ## Roles
//!
//! - **Admin**: set at `initialize`; the only address allowed to call
//!   `execute_payroll` and `set_verifier_vk_hash`.
//! - **Funder**: any address may call `fund_pool` to deposit tokens.
//! - **Auditor**: any address may call `verify_auditor` (read-only).
//!
//! ## Storage layout (Persistent ledger)
//!
//! | Key (Symbol)   | Type       | Description                              |
//! |----------------|------------|------------------------------------------|
//! | `"admin"`      | Address    | Contract administrator                   |
//! | `"verifier"`   | Address    | Groth16 verifier contract address        |
//! | `"token"`      | Address    | SEP-41 token used for the pool (XLM)     |
//! | `"pool_bal"`   | i128       | Current pool balance (in token base unit)|
//! | `"epoch"`      | u64        | Payroll epoch counter                    |
//! | `"vk_hash"`    | BytesN<32> | SHA-256 of the expected VK (optional)    |
//! | `"claim"`      | Address    | Claim contract receiving payroll escrow  |
//! | BytesN<32> key | bool       | Per-nullifier spent flag                 |

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, symbol_short, token, Address, Bytes, BytesN, Env, Vec,
};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum PayrollError {
    /// The contract has already been initialized.
    AlreadyInitialized = 1,
    /// The contract has not been initialized yet.
    NotInitialized = 2,
    /// Caller is not the admin.
    Unauthorized = 3,
    /// One or more nullifiers in the batch have already been spent.
    NullifierSpent = 4,
    /// The ZK proof failed on-chain verification.
    InvalidProof = 5,
    /// Pool does not have enough balance to cover `total_payroll`.
    InsufficientFunds = 6,
    /// `total_payroll` must be a positive integer.
    InvalidAmount = 7,
    /// Claim contract address has not been configured.
    ClaimNotConfigured = 8,
    /// The contract is currently paused by the admin.
    ContractPaused = 9,
    /// No pending admin transfer exists to accept.
    NoPendingAdmin = 10,
    /// Proposed new admin is identical to current admin.
    SameAdmin = 11,
}

// ---------------------------------------------------------------------------
// Storage key helpers (all `symbol_short!` macros — max 9 ASCII chars)
// ---------------------------------------------------------------------------

#[inline]
fn key_admin() -> soroban_sdk::Symbol {
    symbol_short!("admin")
}
#[inline]
fn key_pending_admin() -> soroban_sdk::Symbol {
    symbol_short!("p_admin")
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
fn key_pool_bal() -> soroban_sdk::Symbol {
    symbol_short!("pool_bal")
}
#[inline]
fn key_epoch() -> soroban_sdk::Symbol {
    symbol_short!("epoch")
}
#[inline]
fn key_vk_hash() -> soroban_sdk::Symbol {
    symbol_short!("vk_hash")
}
#[inline]
fn key_claim() -> soroban_sdk::Symbol {
    symbol_short!("claim")
}
#[inline]
fn key_paused() -> soroban_sdk::Symbol {
    symbol_short!("paused")
}

// ---------------------------------------------------------------------------
// Cross-contract client for the Groth16Verifier
// ---------------------------------------------------------------------------

mod verifier_contract {
    use soroban_sdk::{contractclient, Bytes, BytesN, Env, Vec};

    #[contractclient(name = "VerifierClient")]
    pub trait VerifierInterface {
        fn verify(env: Env, proof_bytes: Bytes, public_signals: Vec<BytesN<32>>) -> bool;
    }
}

use verifier_contract::VerifierClient;

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct PayrollContract;

#[contractimpl]
impl PayrollContract {
    // -----------------------------------------------------------------------
    // initialize
    //
    // Must be called exactly once after deployment.
    //
    // `admin`             — address with payroll execution rights
    // `verifier_contract` — deployed Groth16Verifier contract address
    // `token_address`     — SEP-41 token contract for the pool (native XLM or USDC)
    // -----------------------------------------------------------------------
    pub fn initialize(
        env: Env,
        admin: Address,
        verifier_contract: Address,
        token_address: Address,
    ) -> Result<(), PayrollError> {
        if env.storage().persistent().has(&key_admin()) {
            return Err(PayrollError::AlreadyInitialized);
        }

        admin.require_auth();

        env.storage().persistent().set(&key_admin(), &admin);
        env.storage()
            .persistent()
            .set(&key_verifier(), &verifier_contract);
        env.storage().persistent().set(&key_token(), &token_address);
        env.storage().persistent().set(&key_pool_bal(), &0i128);
        env.storage().persistent().set(&key_epoch(), &0u64);

        env.events().publish(
            (symbol_short!("init"),),
            (admin, verifier_contract, token_address),
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // fund_pool
    //
    // Transfer `amount` base units of the pool token from `funder` into this
    // contract and credit the pool balance.
    // -----------------------------------------------------------------------
    pub fn fund_pool(env: Env, funder: Address, amount: i128) -> Result<(), PayrollError> {
        Self::assert_initialized(&env)?;

        if amount <= 0 {
            return Err(PayrollError::InvalidAmount);
        }

        funder.require_auth();

        let token_addr: Address = env
            .storage()
            .persistent()
            .get(&key_token())
            .ok_or(PayrollError::NotInitialized)?;
        let token = token::Client::new(&env, &token_addr);
        token.transfer(&funder, &env.current_contract_address(), &amount);

        let current: i128 = env.storage().persistent().get(&key_pool_bal()).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&key_pool_bal(), &(current + amount));

        env.events()
            .publish((symbol_short!("fund"),), (funder, amount));

        Ok(())
    }

    // -----------------------------------------------------------------------
    // execute_payroll
    //
    // Admin-only.  Verifies a Groth16 proof for a payroll batch, marks all
    // provided nullifiers as spent, and deducts `total_payroll` from the pool.
    //
    // The actual per-employee transfers are handled off-chain or by a separate
    // claim contract; this function is the on-chain gate that guarantees the
    // proof is valid and prevents double-execution via nullifiers.
    //
    // Parameters:
    //   caller         — must equal the stored admin
    //   proof          — 256-byte Groth16 proof blob
    //   public_signals — Vec<BytesN<32>> matching circuit public inputs
    //   nullifiers     — Vec<BytesN<32>> anti-replay tokens (one per employee)
    //   total_payroll  — total amount to deduct from pool
    //
    // Returns `true` on success.
    // -----------------------------------------------------------------------
    pub fn execute_payroll(
        env: Env,
        caller: Address,
        proof: Bytes,
        public_signals: Vec<BytesN<32>>,
        nullifiers: Vec<BytesN<32>>,
        total_payroll: i128,
    ) -> Result<bool, PayrollError> {
        Self::assert_initialized(&env)?;

        if env
            .storage()
            .persistent()
            .get(&key_paused())
            .unwrap_or(false)
        {
            return Err(PayrollError::ContractPaused);
        }

        // Auth + admin check
        caller.require_auth();
        let admin: Address = env
            .storage()
            .persistent()
            .get(&key_admin())
            .ok_or(PayrollError::NotInitialized)?;
        if caller != admin {
            return Err(PayrollError::Unauthorized);
        }

        // Amount sanity
        if total_payroll <= 0 {
            return Err(PayrollError::InvalidAmount);
        }

        // Pool balance check
        let pool_bal: i128 = env.storage().persistent().get(&key_pool_bal()).unwrap_or(0);
        if pool_bal < total_payroll {
            return Err(PayrollError::InsufficientFunds);
        }

        // Nullifier pre-check — reject batch if ANY nullifier is already spent
        for i in 0..nullifiers.len() {
            let nul = nullifiers.get(i).ok_or(PayrollError::InvalidAmount)?;
            let spent: bool = env
                .storage()
                .persistent()
                .get::<BytesN<32>, bool>(&nul)
                .unwrap_or(false);
            if spent {
                return Err(PayrollError::NullifierSpent);
            }
        }

        // ZK proof verification via cross-contract call
        let verifier_addr: Address = env
            .storage()
            .persistent()
            .get(&key_verifier())
            .ok_or(PayrollError::NotInitialized)?;
        let verifier = VerifierClient::new(&env, &verifier_addr);
        let valid = verifier.verify(&proof, &public_signals);
        if !valid {
            return Err(PayrollError::InvalidProof);
        }

        // Commit: mark nullifiers as spent
        for i in 0..nullifiers.len() {
            let nul = nullifiers.get(i).ok_or(PayrollError::InvalidAmount)?;
            env.storage().persistent().set(&nul, &true);
        }

        // Deduct from pool
        let new_bal = pool_bal - total_payroll;
        env.storage().persistent().set(&key_pool_bal(), &new_bal);

        // Move escrow to the claim contract for employee withdrawals
        let claim_addr: Address = env
            .storage()
            .persistent()
            .get(&key_claim())
            .ok_or(PayrollError::ClaimNotConfigured)?;
        let token_addr: Address = env
            .storage()
            .persistent()
            .get(&key_token())
            .ok_or(PayrollError::NotInitialized)?;
        let token = token::Client::new(&env, &token_addr);
        token.transfer(&env.current_contract_address(), &claim_addr, &total_payroll);

        // Increment payroll epoch
        let epoch: u64 = env.storage().persistent().get(&key_epoch()).unwrap_or(0);
        let next_epoch = epoch + 1;
        env.storage().persistent().set(&key_epoch(), &next_epoch);

        env.events()
            .publish((symbol_short!("payroll"),), (epoch, total_payroll, new_bal));

        Ok(true)
    }

    // -----------------------------------------------------------------------
    // verify_auditor
    //
    // Read-only proof check for auditors.  Does NOT modify state, does NOT
    // check nullifiers, does NOT deduct from pool.
    // -----------------------------------------------------------------------
    pub fn verify_auditor(
        env: Env,
        proof: Bytes,
        public_signals: Vec<BytesN<32>>,
    ) -> Result<bool, PayrollError> {
        Self::assert_initialized(&env)?;

        let verifier_addr: Address = env
            .storage()
            .persistent()
            .get(&key_verifier())
            .ok_or(PayrollError::NotInitialized)?;
        let verifier = VerifierClient::new(&env, &verifier_addr);
        Ok(verifier.verify(&proof, &public_signals))
    }

    // -----------------------------------------------------------------------
    // get_pool_balance
    // -----------------------------------------------------------------------
    pub fn get_pool_balance(env: Env) -> Result<i128, PayrollError> {
        Self::assert_initialized(&env)?;
        Ok(env.storage().persistent().get(&key_pool_bal()).unwrap_or(0))
    }

    // -----------------------------------------------------------------------
    // is_nullifier_spent
    // -----------------------------------------------------------------------
    pub fn is_nullifier_spent(env: Env, nullifier: BytesN<32>) -> bool {
        env.storage()
            .persistent()
            .get::<BytesN<32>, bool>(&nullifier)
            .unwrap_or(false)
    }

    // -----------------------------------------------------------------------
    // get_epoch
    // -----------------------------------------------------------------------
    pub fn get_epoch(env: Env) -> Result<u64, PayrollError> {
        Self::assert_initialized(&env)?;
        Ok(env.storage().persistent().get(&key_epoch()).unwrap_or(0))
    }

    // -----------------------------------------------------------------------
    // set_verifier_vk_hash
    //
    // Admin-only.  Stores a SHA-256 digest of the expected VK so the frontend
    // can verify the on-chain VK has not been swapped without fetching all VK
    // bytes.
    // -----------------------------------------------------------------------
    pub fn set_verifier_vk_hash(
        env: Env,
        admin: Address,
        vk_hash: BytesN<32>,
    ) -> Result<(), PayrollError> {
        Self::assert_initialized(&env)?;

        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&key_admin())
            .ok_or(PayrollError::NotInitialized)?;
        if admin != stored_admin {
            return Err(PayrollError::Unauthorized);
        }

        env.storage().persistent().set(&key_vk_hash(), &vk_hash);

        env.events().publish((symbol_short!("vk_hash"),), vk_hash);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // set_claim_contract
    //
    // Admin-only. Stores the claim contract that receives payroll escrow.
    // -----------------------------------------------------------------------
    pub fn set_claim_contract(
        env: Env,
        admin: Address,
        claim_contract: Address,
    ) -> Result<(), PayrollError> {
        Self::assert_initialized(&env)?;

        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&key_admin())
            .ok_or(PayrollError::NotInitialized)?;
        if admin != stored_admin {
            return Err(PayrollError::Unauthorized);
        }

        env.storage()
            .persistent()
            .set(&key_claim(), &claim_contract);

        env.events()
            .publish((symbol_short!("claimset"),), claim_contract);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // set_verifier
    //
    // Admin-only. Rotates the Groth16 verifier contract address.
    // If `new_vk_hash` is provided (Some), updates the stored VK digest.
    // If None is provided, clears the stored VK digest so off-chain clients
    // never see a stale verification key hash for the rotated verifier.
    // -----------------------------------------------------------------------
    pub fn set_verifier(
        env: Env,
        admin: Address,
        verifier_contract: Address,
        new_vk_hash: Option<BytesN<32>>,
    ) -> Result<(), PayrollError> {
        Self::assert_initialized(&env)?;

        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&key_admin())
            .ok_or(PayrollError::NotInitialized)?;
        if admin != stored_admin {
            return Err(PayrollError::Unauthorized);
        }

        let old_verifier: Address = env
            .storage()
            .persistent()
            .get(&key_verifier())
            .ok_or(PayrollError::NotInitialized)?;
        env.storage()
            .persistent()
            .set(&key_verifier(), &verifier_contract);

        match new_vk_hash {
            Some(vk) => {
                env.storage().persistent().set(&key_vk_hash(), &vk);
                env.events().publish((symbol_short!("vk_hash"),), vk);
            }
            None => {
                env.storage().persistent().remove(&key_vk_hash());
            }
        }

        env.events().publish(
            (symbol_short!("set_ver"),),
            (old_verifier, verifier_contract),
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // set_paused
    //
    // Admin-only. Pauses or unpauses payroll batch executions.
    // -----------------------------------------------------------------------
    pub fn set_paused(env: Env, admin: Address, paused: bool) -> Result<(), PayrollError> {
        Self::assert_initialized(&env)?;

        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&key_admin())
            .ok_or(PayrollError::NotInitialized)?;
        if admin != stored_admin {
            return Err(PayrollError::Unauthorized);
        }

        env.storage().persistent().set(&key_paused(), &paused);

        env.events().publish((symbol_short!("pause"),), paused);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // transfer_admin
    //
    // Step 1 of two-step admin rotation. Admin sets `new_admin` as pending.
    // Rejects new_admin == admin to avoid redundant pending states.
    // -----------------------------------------------------------------------
    pub fn transfer_admin(
        env: Env,
        admin: Address,
        new_admin: Address,
    ) -> Result<(), PayrollError> {
        Self::assert_initialized(&env)?;

        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&key_admin())
            .ok_or(PayrollError::NotInitialized)?;
        if admin != stored_admin {
            return Err(PayrollError::Unauthorized);
        }
        if new_admin == admin {
            return Err(PayrollError::SameAdmin);
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
    pub fn cancel_admin_transfer(env: Env, admin: Address) -> Result<(), PayrollError> {
        Self::assert_initialized(&env)?;

        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&key_admin())
            .ok_or(PayrollError::NotInitialized)?;
        if admin != stored_admin {
            return Err(PayrollError::Unauthorized);
        }

        if !env.storage().persistent().has(&key_pending_admin()) {
            return Err(PayrollError::NoPendingAdmin);
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
    pub fn accept_admin(env: Env, new_admin: Address) -> Result<(), PayrollError> {
        Self::assert_initialized(&env)?;

        new_admin.require_auth();
        let pending_admin: Address = env
            .storage()
            .persistent()
            .get(&key_pending_admin())
            .ok_or(PayrollError::NoPendingAdmin)?;
        if new_admin != pending_admin {
            return Err(PayrollError::Unauthorized);
        }

        let old_admin: Address = env
            .storage()
            .persistent()
            .get(&key_admin())
            .ok_or(PayrollError::NotInitialized)?;
        env.storage().persistent().set(&key_admin(), &new_admin);
        env.storage().persistent().remove(&key_pending_admin());

        env.events()
            .publish((symbol_short!("adm_acc"),), (old_admin, new_admin));

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Config getters
    // -----------------------------------------------------------------------

    pub fn get_admin(env: Env) -> Result<Address, PayrollError> {
        Self::assert_initialized(&env)?;
        env.storage()
            .persistent()
            .get(&key_admin())
            .ok_or(PayrollError::NotInitialized)
    }

    pub fn get_pending_admin(env: Env) -> Option<Address> {
        Self::assert_initialized(&env).ok()?;
        env.storage().persistent().get(&key_pending_admin())
    }

    pub fn get_verifier(env: Env) -> Result<Address, PayrollError> {
        Self::assert_initialized(&env)?;
        env.storage()
            .persistent()
            .get(&key_verifier())
            .ok_or(PayrollError::NotInitialized)
    }

    pub fn get_token(env: Env) -> Result<Address, PayrollError> {
        Self::assert_initialized(&env)?;
        env.storage()
            .persistent()
            .get(&key_token())
            .ok_or(PayrollError::NotInitialized)
    }

    pub fn get_claim_contract(env: Env) -> Option<Address> {
        Self::assert_initialized(&env).ok()?;
        env.storage().persistent().get(&key_claim())
    }

    pub fn get_vk_hash(env: Env) -> Option<BytesN<32>> {
        Self::assert_initialized(&env).ok()?;
        env.storage().persistent().get(&key_vk_hash())
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

    // -----------------------------------------------------------------------
    // Note on token immutability:
    // The SEP-41 token address (`token`) is intentionally immutable once set in
    // `initialize`. Modifying the token would break accounting invariants for
    // existing deposited pool balance and escrow, risking unauthorized asset
    // substitution or insolvency. If a different token asset is needed, a new
    // Payroll contract instance must be deployed.
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn assert_initialized(env: &Env) -> Result<(), PayrollError> {
        if !env.storage().persistent().has(&key_admin()) {
            return Err(PayrollError::NotInitialized);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    // -----------------------------------------------------------------------
    // Mock verifier: always returns true
    // -----------------------------------------------------------------------
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

    // -----------------------------------------------------------------------
    // Mock verifier: always returns false
    // -----------------------------------------------------------------------
    mod mock_fail {
        use soroban_sdk::{contract, contractimpl, Bytes, BytesN, Env, Vec};

        #[contract]
        pub struct MockFail;

        #[contractimpl]
        impl MockFail {
            pub fn verify(_env: Env, _proof: Bytes, _signals: Vec<BytesN<32>>) -> bool {
                false
            }
        }
    }

    mod mock_claim {
        use soroban_sdk::{contract, contractimpl, Env};

        #[contract]
        pub struct MockClaim;

        #[contractimpl]
        impl MockClaim {
            pub fn noop(_env: Env) {}
        }
    }

    // -----------------------------------------------------------------------
    // Test setup helper — returns (admin, payroll_id, xlm_id, claim_id)
    // -----------------------------------------------------------------------
    fn setup_ok(env: &Env) -> (Address, Address, Address, Address) {
        let admin = Address::generate(env);
        let xlm_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let verifier_id = env.register(mock_ok::MockOk, ());
        let claim_id = env.register(mock_claim::MockClaim, ());
        let payroll_id = env.register(PayrollContract, ());

        env.mock_all_auths();

        let client = PayrollContractClient::new(env, &payroll_id);
        client.initialize(&admin, &verifier_id, &xlm_id);
        client.set_claim_contract(&admin, &claim_id);

        (admin, payroll_id, xlm_id, claim_id)
    }

    #[test]
    fn test_initialize_sets_initial_state() {
        let env = Env::default();
        let (_admin, payroll_id, _xlm_id, _claim_id) = setup_ok(&env);
        let client = PayrollContractClient::new(&env, &payroll_id);

        assert_eq!(client.get_epoch(), 0u64);
        assert_eq!(client.get_pool_balance(), 0i128);
    }

    #[test]
    fn test_initialize_twice_fails() {
        let env = Env::default();
        let (admin, payroll_id, xlm_id, _claim_id) = setup_ok(&env);
        let client = PayrollContractClient::new(&env, &payroll_id);

        let other_verifier = env.register(mock_ok::MockOk, ());
        let result = client.try_initialize(&admin, &other_verifier, &xlm_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_nullifier_not_spent_initially() {
        let env = Env::default();
        let (_admin, payroll_id, _xlm_id, _claim_id) = setup_ok(&env);
        let client = PayrollContractClient::new(&env, &payroll_id);

        let nul = BytesN::from_array(&env, &[0xABu8; 32]);
        assert!(!client.is_nullifier_spent(&nul));
    }

    #[test]
    fn test_fund_pool_increases_balance() {
        let env = Env::default();
        let (admin, payroll_id, xlm_id, _claim_id) = setup_ok(&env);
        let client = PayrollContractClient::new(&env, &payroll_id);

        let xlm = token::StellarAssetClient::new(&env, &xlm_id);
        xlm.mint(&admin, &1_000_0000000i128);

        client.fund_pool(&admin, &500_0000000i128);
        assert_eq!(client.get_pool_balance(), 500_0000000i128);

        client.fund_pool(&admin, &250_0000000i128);
        assert_eq!(client.get_pool_balance(), 750_0000000i128);
    }

    #[test]
    fn test_execute_payroll_happy_path() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let xlm_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let verifier_id = env.register(mock_ok::MockOk, ());
        let claim_id = env.register(mock_claim::MockClaim, ());
        let payroll_id = env.register(PayrollContract, ());
        let client = PayrollContractClient::new(&env, &payroll_id);
        client.initialize(&admin, &verifier_id, &xlm_id);
        client.set_claim_contract(&admin, &claim_id);

        let xlm = token::StellarAssetClient::new(&env, &xlm_id);
        xlm.mint(&admin, &1_000_0000000i128);
        client.fund_pool(&admin, &500_0000000i128);

        let mut nullifiers: Vec<BytesN<32>> = Vec::new(&env);
        nullifiers.push_back(BytesN::from_array(&env, &[0x01u8; 32]));
        nullifiers.push_back(BytesN::from_array(&env, &[0x02u8; 32]));

        let proof = Bytes::from_slice(&env, &[0u8; 256]);
        let signals: Vec<BytesN<32>> = Vec::new(&env);

        let result =
            client.execute_payroll(&admin, &proof, &signals, &nullifiers, &100_0000000i128);
        assert!(result);

        assert_eq!(client.get_pool_balance(), 400_0000000i128);
        assert_eq!(client.get_epoch(), 1u64);
        assert!(client.is_nullifier_spent(&BytesN::from_array(&env, &[0x01u8; 32])));
        assert!(client.is_nullifier_spent(&BytesN::from_array(&env, &[0x02u8; 32])));
    }

    #[test]
    fn test_execute_payroll_rejects_spent_nullifier() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let xlm_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let verifier_id = env.register(mock_ok::MockOk, ());
        let claim_id = env.register(mock_claim::MockClaim, ());
        let payroll_id = env.register(PayrollContract, ());
        let client = PayrollContractClient::new(&env, &payroll_id);
        client.initialize(&admin, &verifier_id, &xlm_id);
        client.set_claim_contract(&admin, &claim_id);

        let xlm = token::StellarAssetClient::new(&env, &xlm_id);
        xlm.mint(&admin, &10_000_0000000i128);
        client.fund_pool(&admin, &5_000_0000000i128);

        let nul = BytesN::from_array(&env, &[0xFFu8; 32]);
        let mut nullifiers: Vec<BytesN<32>> = Vec::new(&env);
        nullifiers.push_back(nul.clone());

        let proof = Bytes::from_slice(&env, &[0u8; 256]);
        let signals: Vec<BytesN<32>> = Vec::new(&env);

        // First call: should succeed
        client.execute_payroll(&admin, &proof, &signals, &nullifiers, &10_0000000i128);

        // Second call with same nullifier: must fail
        let result =
            client.try_execute_payroll(&admin, &proof, &signals, &nullifiers, &10_0000000i128);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_payroll_rejects_invalid_proof() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let xlm_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let verifier_id = env.register(mock_fail::MockFail, ());
        let claim_id = env.register(mock_claim::MockClaim, ());
        let payroll_id = env.register(PayrollContract, ());
        let client = PayrollContractClient::new(&env, &payroll_id);
        client.initialize(&admin, &verifier_id, &xlm_id);
        client.set_claim_contract(&admin, &claim_id);

        let xlm = token::StellarAssetClient::new(&env, &xlm_id);
        xlm.mint(&admin, &1_000_0000000i128);
        client.fund_pool(&admin, &500_0000000i128);

        let nullifiers: Vec<BytesN<32>> = Vec::new(&env);
        let proof = Bytes::from_slice(&env, &[0u8; 256]);
        let signals: Vec<BytesN<32>> = Vec::new(&env);

        let result =
            client.try_execute_payroll(&admin, &proof, &signals, &nullifiers, &100_0000000i128);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_payroll_rejects_insufficient_funds() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let xlm_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let verifier_id = env.register(mock_ok::MockOk, ());
        let claim_id = env.register(mock_claim::MockClaim, ());
        let payroll_id = env.register(PayrollContract, ());
        let client = PayrollContractClient::new(&env, &payroll_id);
        client.initialize(&admin, &verifier_id, &xlm_id);
        client.set_claim_contract(&admin, &claim_id);

        let xlm = token::StellarAssetClient::new(&env, &xlm_id);
        xlm.mint(&admin, &100_0000000i128);
        client.fund_pool(&admin, &100_0000000i128);

        let nullifiers: Vec<BytesN<32>> = Vec::new(&env);
        let proof = Bytes::from_slice(&env, &[0u8; 256]);
        let signals: Vec<BytesN<32>> = Vec::new(&env);

        // Request more than pool holds
        let result =
            client.try_execute_payroll(&admin, &proof, &signals, &nullifiers, &999_0000000i128);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_payroll_rejects_non_admin() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let attacker = Address::generate(&env);
        let xlm_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let verifier_id = env.register(mock_ok::MockOk, ());
        let claim_id = env.register(mock_claim::MockClaim, ());
        let payroll_id = env.register(PayrollContract, ());
        let client = PayrollContractClient::new(&env, &payroll_id);
        client.initialize(&admin, &verifier_id, &xlm_id);
        client.set_claim_contract(&admin, &claim_id);

        let xlm = token::StellarAssetClient::new(&env, &xlm_id);
        xlm.mint(&admin, &1_000_0000000i128);
        client.fund_pool(&admin, &500_0000000i128);

        let nullifiers: Vec<BytesN<32>> = Vec::new(&env);
        let proof = Bytes::from_slice(&env, &[0u8; 256]);
        let signals: Vec<BytesN<32>> = Vec::new(&env);

        let result =
            client.try_execute_payroll(&attacker, &proof, &signals, &nullifiers, &10_0000000i128);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_auditor_passes_through() {
        let env = Env::default();
        let (_admin, payroll_id, _xlm_id, _claim_id) = setup_ok(&env);
        let client = PayrollContractClient::new(&env, &payroll_id);

        let proof = Bytes::from_slice(&env, &[0u8; 256]);
        let signals: Vec<BytesN<32>> = Vec::new(&env);
        // MockOk always returns true
        assert!(client.verify_auditor(&proof, &signals));
    }

    #[test]
    fn test_set_verifier_vk_hash_admin_only() {
        let env = Env::default();
        let (admin, payroll_id, _xlm_id, _claim_id) = setup_ok(&env);
        let client = PayrollContractClient::new(&env, &payroll_id);

        let hash = BytesN::from_array(&env, &[0xDEu8; 32]);
        // Admin call — should succeed
        client.set_verifier_vk_hash(&admin, &hash);

        // Non-admin — should fail
        let attacker = Address::generate(&env);
        let result = client.try_set_verifier_vk_hash(&attacker, &hash);
        assert!(result.is_err());
    }

    #[test]
    fn test_fund_pool_rejects_zero_amount() {
        let env = Env::default();
        let (admin, payroll_id, _xlm_id, _claim_id) = setup_ok(&env);
        let client = PayrollContractClient::new(&env, &payroll_id);

        let result = client.try_fund_pool(&admin, &0i128);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_getters() {
        let env = Env::default();
        let (admin, payroll_id, xlm_id, claim_id) = setup_ok(&env);
        let client = PayrollContractClient::new(&env, &payroll_id);

        assert_eq!(client.get_admin(), admin);
        assert_eq!(client.get_token(), xlm_id);
        assert_eq!(client.get_claim_contract(), Some(claim_id));
        assert_eq!(client.get_pending_admin(), None);
        assert_eq!(client.is_paused(), false);
        assert_eq!(client.get_vk_hash(), None);

        // Setting vk_hash updates getter
        let hash = BytesN::from_array(&env, &[0xAAu8; 32]);
        client.set_verifier_vk_hash(&admin, &hash);
        assert_eq!(client.get_vk_hash(), Some(hash));
    }

    #[test]
    fn test_set_verifier_happy_and_unauthorized() {
        let env = Env::default();
        let (admin, payroll_id, _xlm_id, _claim_id) = setup_ok(&env);
        let client = PayrollContractClient::new(&env, &payroll_id);

        let new_verifier = env.register(mock_ok::MockOk, ());
        let attacker = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[0xBBu8; 32]);

        // Non-admin rejected
        let fail_res = client.try_set_verifier(&attacker, &new_verifier, &Some(hash.clone()));
        assert!(fail_res.is_err());

        // Admin succeeds and updates vk_hash
        client.set_verifier(&admin, &new_verifier, &Some(hash.clone()));
        assert_eq!(client.get_verifier(), new_verifier);
        assert_eq!(client.get_vk_hash(), Some(hash));

        // Setting another verifier with None clears stale vk_hash
        let another_verifier = env.register(mock_ok::MockOk, ());
        client.set_verifier(&admin, &another_verifier, &None);
        assert_eq!(client.get_verifier(), another_verifier);
        assert_eq!(client.get_vk_hash(), None);
    }

    #[test]
    fn test_set_paused_halts_and_resumes_payroll() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let attacker = Address::generate(&env);
        let xlm_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let verifier_id = env.register(mock_ok::MockOk, ());
        let claim_id = env.register(mock_claim::MockClaim, ());
        let payroll_id = env.register(PayrollContract, ());
        let client = PayrollContractClient::new(&env, &payroll_id);
        client.initialize(&admin, &verifier_id, &xlm_id);
        client.set_claim_contract(&admin, &claim_id);

        let xlm = token::StellarAssetClient::new(&env, &xlm_id);
        xlm.mint(&admin, &1_000_0000000i128);
        client.fund_pool(&admin, &500_0000000i128);

        let mut nullifiers: Vec<BytesN<32>> = Vec::new(&env);
        nullifiers.push_back(BytesN::from_array(&env, &[0x11u8; 32]));
        let proof = Bytes::from_slice(&env, &[0u8; 256]);
        let signals: Vec<BytesN<32>> = Vec::new(&env);

        // Non-admin cannot pause
        let unauth = client.try_set_paused(&attacker, &true);
        assert!(unauth.is_err());

        // Admin pauses
        client.set_paused(&admin, &true);
        assert!(client.is_paused());

        // While paused, execute_payroll must fail with ContractPaused
        let paused_err =
            client.try_execute_payroll(&admin, &proof, &signals, &nullifiers, &50_0000000i128);
        assert_eq!(paused_err, Err(Ok(PayrollError::ContractPaused)));

        // Unpause
        client.set_paused(&admin, &false);
        assert!(!client.is_paused());

        // execute_payroll succeeds after unpause
        let ok_res = client.execute_payroll(&admin, &proof, &signals, &nullifiers, &50_0000000i128);
        assert!(ok_res);
    }

    #[test]
    fn test_two_step_admin_transfer_full_lifecycle() {
        let env = Env::default();
        let (admin, payroll_id, _xlm_id, _claim_id) = setup_ok(&env);
        let client = PayrollContractClient::new(&env, &payroll_id);

        let new_admin = Address::generate(&env);
        let attacker = Address::generate(&env);

        // Transfer to self rejected
        let same_admin = client.try_transfer_admin(&admin, &admin);
        assert_eq!(same_admin, Err(Ok(PayrollError::SameAdmin)));

        // Non-admin cannot initiate transfer
        let unauth_transfer = client.try_transfer_admin(&attacker, &new_admin);
        assert!(unauth_transfer.is_err());

        // Calling accept_admin when no transfer is pending fails
        let no_pending = client.try_accept_admin(&new_admin);
        assert_eq!(no_pending, Err(Ok(PayrollError::NoPendingAdmin)));

        // Admin initiates transfer
        client.transfer_admin(&admin, &new_admin);
        assert_eq!(client.get_pending_admin(), Some(new_admin.clone()));
        assert_eq!(client.get_admin(), admin);

        // Admin cancels transfer
        client.cancel_admin_transfer(&admin);
        assert_eq!(client.get_pending_admin(), None);

        // Re-initiate transfer
        client.transfer_admin(&admin, &new_admin);

        // Attacker cannot accept admin
        let attacker_accept = client.try_accept_admin(&attacker);
        assert!(attacker_accept.is_err());

        // Designated new_admin accepts admin
        client.accept_admin(&new_admin);
        assert_eq!(client.get_admin(), new_admin);
        assert_eq!(client.get_pending_admin(), None);

        // Old admin can no longer perform admin actions
        let old_admin_action = client.try_set_paused(&admin, &true);
        assert!(old_admin_action.is_err());

        // New admin can perform admin actions
        client.set_paused(&new_admin, &true);
        assert!(client.is_paused());
    }

    #[test]
    fn test_payroll_admin_auth_verification() {
        let env = Env::default();
        let (admin, payroll_id, _xlm_id, claim_id) = setup_ok(&env);
        let client = PayrollContractClient::new(&env, &payroll_id);

        let candidate = Address::generate(&env);
        let new_ver = env.register(mock_ok::MockOk, ());
        let vk_hash = BytesN::from_array(&env, &[0xAAu8; 32]);

        // 1. set_paused
        client.set_paused(&admin, &true);
        assert_eq!(env.auths()[0].0, admin);
        client.set_paused(&admin, &false);

        // 2. transfer_admin
        client.transfer_admin(&admin, &candidate);
        assert_eq!(env.auths()[0].0, admin);

        // 3. cancel_admin_transfer
        client.cancel_admin_transfer(&admin);
        assert_eq!(env.auths()[0].0, admin);

        // 4. set_verifier
        client.set_verifier(&admin, &new_ver, &Some(vk_hash.clone()));
        assert_eq!(env.auths()[0].0, admin);

        // 5. set_verifier_vk_hash
        client.set_verifier_vk_hash(&admin, &vk_hash);
        assert_eq!(env.auths()[0].0, admin);

        // 6. set_claim_contract
        client.set_claim_contract(&admin, &claim_id);
        assert_eq!(env.auths()[0].0, admin);
    }
}
