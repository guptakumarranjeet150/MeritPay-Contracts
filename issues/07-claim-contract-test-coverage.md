# `claim` contract has only a single happy-path test

**Labels:** testing, medium, contracts
**Component:** `contracts/claim/src/lib.rs`

## Problem / context

`ClaimContract` guards real escrowed funds, yet its entire test module is one
test: `test_claim_payout_happy_path`. Every error path and branch is untested:

| Branch / error | Covered? |
|---|---|
| `NotInitialized` (call before init) | ❌ |
| `InvalidAmount` (`amount <= 0`) | ❌ |
| `SignalMismatch` — `public_signals.len() != 3` | ❌ |
| `SignalMismatch` — `public_signals[0] != nullifier` | ❌ |
| `SignalMismatch` — `public_signals[2] != amount` | ❌ |
| `NullifierNotAuthorized` — nullifier not spent in payroll | ❌ |
| `AlreadyClaimed` — second claim on same nullifier | ❌ |
| `PayrollEpochNotExecuted` — `on_chain_epoch < proof_epoch` | ❌ |
| `InvalidProof` — verifier returns false (`MockFail`) | ❌ |
| `AlreadyInitialized` — double init | ❌ |
| `is_claimed` view before/after | partial |
| `bytes_to_u64` / `bytes_to_i128` decoding (byte offsets, big-endian) | ❌ |

`bytes_to_i128` reading `arr[16..32]` and `bytes_to_u64` reading `arr[24..32]`
are exactly the kind of off-by-one/endianness code that needs direct unit tests.

## Proposed solution

Add a `MockFail` verifier (copy from the payroll tests) and a configurable
`MockPayroll` (parameterize `is_nullifier_spent` / `get_epoch` return values, or
add small setter methods) so each branch can be driven, then write one test per
row above.

Add direct unit tests for `bytes_to_u64` / `bytes_to_i128` covering: zero,
small value, max value, a value with high bytes set (documenting the truncation
behavior — see issue #10-adjacent), and round-trip against the encoding the
frontend/circuit produces.

## Scope / requirements

- `MockFail` + flexible `MockPayroll` test doubles in `claim`'s test module.
- ~10 new `#[test]` functions.
- Optional: extract `bytes_to_u64` / `bytes_to_i128` so they are unit-testable
  without a contract env, or test via a thin exposed test-only wrapper.

## Acceptance criteria

- [ ] Every variant of `ClaimError` has at least one test that triggers it.
- [ ] Second `claim_payout` with the same nullifier returns `AlreadyClaimed` and
      transfers no additional tokens.
- [ ] `bytes_to_i128` / `bytes_to_u64` have direct assertions for at least 4
      input vectors each.
- [ ] `cargo test -p claim` reports the new tests passing.

## Relevant files

- `contracts/claim/src/lib.rs` (tests module)
- `contracts/payroll/src/lib.rs` (`MockFail` reference)

## Estimated difficulty

**Low–medium.** Repetitive but mechanical; the test doubles already mostly exist.
