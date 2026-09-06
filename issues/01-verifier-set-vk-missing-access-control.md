# `groth16_verifier.set_vk` has no access control — anyone can swap the verification key

**Labels:** security, critical, contracts
**Component:** `contracts/groth16_verifier/src/lib.rs`

## Problem / context

`Groth16Verifier::set_vk` writes the verification key to persistent storage with
**no authorization check of any kind**:

```rust
pub fn set_vk(env: Env, vk_bytes: Bytes) -> Result<(), VerifierError> {
    // ... length checks only ...
    env.storage().persistent().set(&symbol_short!("VK"), &StoredVk { raw: vk_bytes });
    Ok(())
}
```

There is no `require_auth`, no stored owner/admin, and no "set once" guard. Any
account on the network can call `set_vk` at any time and replace the VK on a
live verifier instance.

Both value-bearing contracts (`payroll` and `claim`) delegate all proof checking
to a `groth16_verifier` instance via cross-contract call. If an attacker replaces
the stored VK with one whose trapdoor they know (or a VK for a trivially
satisfiable circuit), they can forge a proof that passes `verify(...)` and then:

- call `payroll.execute_payroll` with attacker-chosen `nullifiers` / `total_payroll`
  and drain the pool into the claim escrow, and/or
- call `claim.claim_payout` and drain the escrow to their own wallet.

The `payroll.set_verifier_vk_hash` function hints that VK integrity was a known
concern, but nothing enforces it on-chain (see separate issue), and the verifier
itself is completely open.

## Proposed solution

Gate `set_vk` behind an owner that is bound at deploy/init time.

1. Add an `initialize(owner: Address)` (or accept the owner as an argument on the
   first `set_vk`) that stores an `owner` address in instance storage and calls
   `owner.require_auth()`.
2. On every subsequent `set_vk`, load `owner`, call `owner.require_auth()`, and
   reject otherwise with a new `VerifierError::Unauthorized`.
3. Decide and document the rotation policy:
   - **Recommended:** allow the owner to rotate the VK (needed for circuit
     upgrades) but emit an event (`vk_set`) on every change so downstream
     contracts / indexers can react.
   - Optionally support a one-way `lock()` that permanently freezes the VK.
4. Update `scripts/deploy.sh` and `scripts/upload_vk.js` to pass/authenticate the
   owner (the deployer identity).
5. Update `payroll` / `claim` deployment wiring docs if the verifier now needs an
   explicit `initialize` step.

## Scope / requirements

- New error variant + owner storage key in `groth16_verifier`.
- Auth enforcement on `set_vk`.
- Event emission on VK change.
- Deploy script + `upload_vk.js` updated to run under the deployer identity
  (they already invoke as `--source-account "$DEPLOYER_KEY_NAME"`, so mainly the
  contract side and an `initialize` call are needed).
- Tests (see acceptance criteria).

## Acceptance criteria

- [ ] A fresh verifier requires an `initialize`/owner-binding step before or
      during the first `set_vk`.
- [ ] `set_vk` called by a non-owner address returns `Unauthorized` and does not
      mutate storage.
- [ ] `set_vk` called by the owner succeeds and emits a `vk_set` event.
- [ ] Existing `set_vk`/`verify` tests updated to perform the owner auth step.
- [ ] New tests: `test_set_vk_rejects_non_owner`, `test_set_vk_allows_owner_rotation`.
- [ ] `README.md` "Security notes" and "Deploying to testnet" updated to describe
      verifier ownership.

## Relevant files

- `contracts/groth16_verifier/src/lib.rs` (`set_vk`, `VerifierError`, tests)
- `scripts/deploy.sh` (sections 5, 6b, 7a)
- `scripts/upload_vk.js`
- `README.md`

## Estimated difficulty

**Low–medium.** Small, well-contained contract change; most effort is updating
the deploy flow and tests.
