# `claim_payout` marks the nullifier claimed *after* transferring tokens (checks-effects-interactions violation)

**Labels:** security, medium, contracts
**Component:** `contracts/claim/src/lib.rs`

## Problem / context

In `ClaimContract::claim_payout` the token transfer happens **before** the
nullifier is recorded as claimed:

```rust
let token = token::Client::new(&env, &token_addr);
token.transfer(&env.current_contract_address(), &recipient, &amount);   // interaction

env.storage().persistent().set(&nullifier, &true);                      // effect (too late)
```

`token_address` is arbitrary contract state chosen at `initialize` time (and per
issue #03 it can currently be set by anyone). A malicious or non-standard SEP-41
token whose `transfer` re-enters `claim_payout` with the same `nullifier` would
pass the `is_claimed` check again (still `false`), producing a **double (or
N-times) payout** until the escrow is drained.

By contrast, `payroll.execute_payroll` correctly marks nullifiers spent *before*
`token.transfer`. `claim_payout` should follow the same ordering.

## Proposed solution

Reorder `claim_payout` to checks → effects → interactions:

1. All validation (auth, signal checks, `is_nullifier_spent`, `is_claimed`,
   epoch, amount, proof verification).
2. **Write** `env.storage().persistent().set(&nullifier, &true)`.
3. **Then** `token.transfer(...)`.
4. Emit the `claim` event.

Optionally, also add an explicit escrow-balance check before transfer that
returns the already-declared-but-unused `ClaimError::InsufficientEscrow` instead
of letting `token.transfer` panic (see issue #12).

## Scope / requirements

- Reorder the final three statements of `claim_payout`.
- Add a regression test that uses a reentrant mock token and asserts a single
  payout.

## Acceptance criteria

- [ ] The nullifier is persisted as claimed before any external token call.
- [ ] New test `test_claim_payout_reentrancy_is_blocked` with a mock token whose
      `transfer` calls back into `claim_payout` — the second call fails with
      `AlreadyClaimed` and total tokens out == `amount`.
- [ ] Existing `test_claim_payout_happy_path` still passes.

## Relevant files

- `contracts/claim/src/lib.rs` (`claim_payout`, tests)

## Estimated difficulty

**Low.** Few-line reorder; the value is the regression test.
