# Contract `initialize` functions can be front-run — `claim` has no auth at all

**Labels:** security, high, contracts
**Component:** `contracts/claim/src/lib.rs`, `contracts/payroll/src/lib.rs`

## Problem / context

`scripts/deploy.sh` deploys each contract and then initializes it in a
**separate transaction**:

```
deploy claim  ->  ...  ->  claim.initialize(payroll, verifier, token)
```

`ClaimContract::initialize` performs **no authorization**:

```rust
pub fn initialize(env: Env, payroll_contract: Address, verifier_contract: Address, token_address: Address)
    -> Result<(), ClaimError>
{
    if env.storage().persistent().has(&key_payroll()) { return Err(AlreadyInitialized); }
    // ... stores all three addresses, no require_auth ...
}
```

Between deployment and the deployer's `initialize` call, **anyone** can call
`initialize` first with:

- a `verifier_contract` they control that always returns `true`, and/or
- a `token_address` they control,

then wait for the pool escrow to arrive and drain it via `claim_payout`.
`AlreadyInitialized` then locks the legitimate deployer out.

`PayrollContract::initialize` calls `admin.require_auth()`, which is better, but
still lets an attacker initialize with **themselves** as `admin` (they satisfy
their own auth), taking over `execute_payroll` and `set_claim_contract`.

## Proposed solution

Bind initialization to a known deployer address that is fixed at construction
time, so it cannot be front-run:

**Option A (recommended): constructor args.** Soroban supports contract
constructors (`__constructor`). Move the init parameters into the constructor so
they are set atomically at deploy time in the same transaction, and drop the
separate `initialize` entrypoint.

**Option B: deployer-gated `initialize`.** Pass an `admin`/`deployer` address,
require `admin.require_auth()` in `claim.initialize` too, and have `deploy.sh`
deploy + initialize in a single bundled transaction (`stellar contract deploy`
supports `--` constructor args, or use a deploying "factory" contract).

Either way:

- `claim.initialize` must require auth from a caller-supplied admin.
- The deploy script must not leave an initialization gap.
- Consider storing an `admin` on `claim` as well so misconfiguration
  (`set_payroll_contract`, `set_verifier`) can be corrected without redeploying.

## Scope / requirements

- Add auth to `claim.initialize` (and ideally an `admin` field).
- Convert both contracts to constructor-based init, **or** bundle deploy+init.
- Update `scripts/deploy.sh` (sections 6c, 7b, 7c).
- Update `README.md` deployment section.

## Acceptance criteria

- [ ] `claim` cannot be initialized by an arbitrary unauthenticated caller.
- [ ] There is no window in the documented deploy flow where a third party can
      initialize either contract before the deployer.
- [ ] Test: `claim.initialize` without the admin signature fails.
- [ ] Test: second `initialize` still fails with `AlreadyInitialized`.
- [ ] `deploy.sh` runs green end-to-end against testnet with the new flow.

## Relevant files

- `contracts/claim/src/lib.rs` (`initialize`, `ClaimError`)
- `contracts/payroll/src/lib.rs` (`initialize`)
- `scripts/deploy.sh`
- `README.md`

## Estimated difficulty

**Medium.** Constructor migration touches all three contracts and the deploy
script; testing is simple.
