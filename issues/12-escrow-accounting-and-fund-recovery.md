# No escrow accounting or fund-recovery path — pool/escrow funds can be permanently stranded

**Labels:** bug, medium, contracts, funds-safety
**Component:** `contracts/payroll/src/lib.rs`, `contracts/claim/src/lib.rs`

## Problem / context

Several gaps around fund custody:

1. **No withdraw from `payroll`.** `fund_pool` is permissionless and the only way
   funds leave the pool is `execute_payroll` → claim escrow. If the uploaded VK
   is wrong, the circuit is changed, or the admin key is lost, every `XLM` in the
   pool is **permanently locked**. There is no `admin_withdraw` / emergency
   drain.

2. **No per-epoch escrow accounting in `claim`.** `execute_payroll` transfers
   `total_payroll` into the claim contract, but `claim` tracks nothing about how
   much is owed for which epoch. Claims draw the token balance down freely. If
   `total_payroll` (from the batch proof) ≠ the exact sum of the individual
   `claim_payout` amounts for that batch — due to a circuit/frontend bug, or a
   `bonus` rounding mismatch between `payroll_aggregator` and `claim` circuits —
   the escrow drifts: either funds are stranded in `claim` forever, or one
   epoch's claimants can drain tokens meant for another epoch's employees.

3. **`ClaimError::InsufficientEscrow` is declared but never used.** `claim_payout`
   calls `token.transfer` with no balance pre-check, so an underfunded escrow
   produces a raw host panic instead of a clean error.

## Proposed solution

- Add `payroll.admin_withdraw(admin, to, amount)` (admin-auth) that returns pool
  tokens and debits `pool_bal`, with an event. Optionally a timelock/pause guard.
- Track escrow obligations in `claim`: on a new `notify_escrow(epoch, amount)`
  call from `payroll` (or have `claim` read it), maintain `total_escrowed` and
  `total_claimed`; reject `claim_payout` that would exceed the outstanding
  balance and add `claim.admin_sweep` for dust left after an epoch fully settles.
- Add an explicit balance/obligation check in `claim_payout` returning
  `InsufficientEscrow`.
- Document the invariant: `sum(claim amounts for epoch e) == total_payroll(e)`
  and what enforces it (the circuits — cross-reference issue #13).

## Scope / requirements

- New admin functions on both contracts (auth + events).
- Escrow obligation counters in `claim` and a `payroll → claim` notification, or
  a pull model where `claim` reads a per-epoch amount from `payroll`.
- Use `InsufficientEscrow`.
- Tests for withdraw auth, over-claim rejection, and dust sweep.

## Acceptance criteria

- [ ] An admin can recover pool funds via a gated `admin_withdraw`; a non-admin
      cannot.
- [ ] `claim_payout` returns `InsufficientEscrow` (not a panic) when the escrow
      cannot cover `amount`.
- [ ] Claims cannot collectively withdraw more than what `execute_payroll`
      escrowed.
- [ ] Tests cover withdraw gating, over-claim, and post-epoch sweep.
- [ ] README documents the escrow invariant and the recovery path.

## Relevant files

- `contracts/payroll/src/lib.rs`
- `contracts/claim/src/lib.rs`
- `README.md`

## Estimated difficulty

**Medium–high.** The withdraw function is easy; robust per-epoch escrow
accounting is a design task.
