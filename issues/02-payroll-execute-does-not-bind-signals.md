# `payroll.execute_payroll` never checks `nullifiers` / `total_payroll` against the verified `public_signals`

**Labels:** security, high, contracts
**Component:** `contracts/payroll/src/lib.rs`

## Problem / context

`execute_payroll` takes `proof`, `public_signals`, `nullifiers`, and
`total_payroll` as **four independent arguments**:

```rust
pub fn execute_payroll(
    env: Env,
    caller: Address,
    proof: Bytes,
    public_signals: Vec<BytesN<32>>,
    nullifiers: Vec<BytesN<32>>,
    total_payroll: i128,
) -> Result<bool, PayrollError>
```

It verifies the Groth16 proof against `public_signals`, then:

- marks every entry of `nullifiers` as spent, and
- deducts `total_payroll` from the pool and transfers it to the claim contract.

But it **never checks that `nullifiers` and `total_payroll` are the same values
that appear in `public_signals`.** Per `README.md`, the PayrollAggregator circuit
exposes `totalPayroll` at public-signal index 10 and `nullifiers[5]` at indices
12–16 — the contract ignores those indices entirely. It also never checks
`public_signals.len()`.

Consequences:

- The verified proof is decoupled from the state transition. A caller can present
  a valid proof for one batch while passing an unrelated `nullifiers` vector
  and/or a `total_payroll` that does not match the proof.
- The nullifier replay defense is only as good as the caller's honesty about
  which nullifiers to burn — a caller can under-report nullifiers (burn fewer
  than the batch actually contains) so the same employee can be paid again in a
  later batch, or over-report to grief other batches.
- `total_payroll` can be set below the proven sum (stranding pool funds in
  escrow with no matching claims) or the proof's `totalPayroll` can differ from
  what is actually moved.

Today `execute_payroll` is `admin`-only, which limits the blast radius to a
compromised/buggy admin or frontend — but the contract is supposed to be the
trust boundary, and the `claim` contract already does this correctly
(`public_signals[0] == nullifier`, `public_signals[2] == amount`). `payroll`
should too.

## Proposed solution

Derive the state-changing values **from** `public_signals` instead of trusting
separate arguments:

1. Assert `public_signals.len() == 17` (the PayrollAggregator public-signal
   count) — return a new `PayrollError::MalformedSignals` otherwise.
2. Read `total_payroll` from `public_signals[10]` (decode big-endian → `i128`,
   with an explicit range/overflow check).
3. Read the 5 nullifiers from `public_signals[12..=16]`.
4. Remove the redundant `nullifiers` / `total_payroll` parameters, or keep them
   only as a caller-supplied cross-check that must exactly equal the decoded
   values (return an error on mismatch).
5. Keep the existing "reject if any nullifier already spent" pre-check, now over
   the decoded nullifiers.
6. Add a shared byte-decoding helper (the `claim` contract's `bytes_to_i128` /
   `bytes_to_u64` could be promoted to a small shared module or duplicated with
   tests).

## Scope / requirements

- Signal-count validation + index-based extraction in `execute_payroll`.
- New error variant(s).
- Interface change (parameter removal or cross-check) — update `scripts/deploy.sh`
  usage docs and the frontend contract in `README.md`.
- Bounds checking on the decoded `i128` amount (reject negative / oversized).

## Acceptance criteria

- [ ] `execute_payroll` rejects a `public_signals` vector whose length != 17.
- [ ] The amount deducted from the pool and the nullifiers burned are taken from
      `public_signals`, not from free-standing arguments.
- [ ] A test proves that passing a mismatched `total_payroll`/`nullifiers`
      argument (if the param is retained as a cross-check) is rejected.
- [ ] A test proves the decoded amount is rejected when negative or exceeds a
      sane maximum.
- [ ] Existing tests updated to build a realistic 17-element `public_signals`
      vector rather than `Vec::new()`.
- [ ] `README.md` "`payroll`" table and "Two-step payroll flow" updated.

## Relevant files

- `contracts/payroll/src/lib.rs` (`execute_payroll`, `PayrollError`, tests)
- `contracts/claim/src/lib.rs` (`bytes_to_i128` / `bytes_to_u64` — candidate for reuse)
- `README.md`
- `scripts/deploy.sh`

## Estimated difficulty

**Medium.** Contract logic is straightforward, but it is an interface change that
ripples into tests, the deploy script, and the frontend integration contract.
