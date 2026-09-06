//! MeritPay Claim Contract
//!
//! Holds payroll escrow released by the payroll contract after a verified batch.
//! Employees claim individual payouts by presenting a Groth16 claim proof tied
//! to their nullifier from the payroll batch.

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, symbol_short,
    token, Address, Bytes, BytesN, Env, Vec,
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

mod verifier_contract {
    use soroban_sdk::{contractclient, Bytes, BytesN, Env, Vec};

    #[contractclient(name = "VerifierClient")]
    pub trait VerifierInterface {
        fn verify(env: Env, proof_bytes: Bytes, public_signals: Vec<BytesN<32>>) -> bool;
    }
}

mod payroll_contract {
    use soroban_sdk::{contractclient, Address, BytesN, Env};

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
        if env.storage().persistent().has(&key_payroll()) {
            return Err(ClaimError::AlreadyInitialized);
        }

        admin.require_auth();

        env.storage()
            .persistent()
            .set(&key_payroll(), &payroll_contract);
        env.storage()
            .persistent()
            .set(&key_verifier(), &verifier_contract);
        env.storage()
            .persistent()
            .set(&key_token(), &token_address);

        env.events().publish(
            (symbol_short!("init"),),
            (payroll_contract, verifier_contract, token_address),
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

        if amount <= 0 {
            return Err(ClaimError::InvalidAmount);
        }

        recipient.require_auth();

        if public_signals.len() != 3 {
            return Err(ClaimError::SignalMismatch);
        }
        
        let first_signal = public_signals
            .get(0)
            .ok_or(ClaimError::SignalMismatch)?;
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

        let on_chain_epoch = payroll.get_epoch();
        let proof_epoch_bytes = public_signals
            .get(1)
            .ok_or(ClaimError::SignalMismatch)?;
        let proof_epoch = Self::bytes_to_u64(&proof_epoch_bytes)?;
        if on_chain_epoch < proof_epoch {
            return Err(ClaimError::PayrollEpochNotExecuted);
        }

        let proof_amount_bytes = public_signals
            .get(2)
            .ok_or(ClaimError::SignalMismatch)?;
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

        env.events().publish(
            (symbol_short!("claim"),),
            (recipient, nullifier, amount),
        );

        Ok(true)
    }

    pub fn is_claimed(env: Env, nullifier: BytesN<32>) -> bool {
        env.storage()
            .persistent()
            .get::<BytesN<32>, bool>(&nullifier)
            .unwrap_or(false)
    }

    fn assert_initialized(env: &Env) -> Result<(), ClaimError> {
        if !env.storage().persistent().has(&key_payroll()) {
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
    use soroban_sdk::{testutils::Address as _, Env};

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

    fn setup(env: &Env) -> (Address, Address, Address, Address) {
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

        (recipient, claim_id, xlm_id, payroll_id)
    }

    #[test]
    fn test_claim_payout_happy_path() {
        let env = Env::default();
        let (recipient, claim_id, _xlm_id, _payroll_id) = setup(&env);
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
        let result = client.claim_payout(
            &recipient,
            &proof,
            &signals,
            &nullifier,
            &10_0000000i128,
        );
        assert!(result);
        assert!(client.is_claimed(&nullifier));
    }
}
