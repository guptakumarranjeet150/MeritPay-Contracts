# Contracts never extend TTL on persistent storage — config and nullifiers can expire

**Labels:** bug, medium, contracts, reliability
**Component:** all three contracts

## Problem / context

Every contract stores everything in `env.storage().persistent()` and never calls
`extend_ttl` on any entry:

- `payroll`: `admin`, `verifier`, `token`, `pool_bal`, `epoch`, `vk_hash`,
  `claim`, and one entry **per spent nullifier**.
- `claim`: `payroll`, `verifier`, `token`, and one entry **per claimed nullifier**.
- `groth16_verifier`: the `VK` blob.

Soroban persistent entries have a TTL and become **archived** once it lapses;
accessing an archived entry fails until it is restored (a separate, awkward
operation). For a system explicitly designed to run across multiple payroll
**epochs** over time, this is a real reliability problem:

- The `VK` entry could archive between deploy and first use, bricking every
  proof verification.
- `admin` / `token` / `claim` archiving bricks `execute_payroll`.
- A nullifier written months ago (during batch execution) could archive before
  the employee claims, causing `claim`'s `is_nullifier_spent` cross-call to see
  it as **not spent** (if archived reads surface as "missing") → the claim is
  rejected, or worse, replay protection weakens depending on host semantics.

The correct pattern is to bump TTL on read/write of long-lived keys, and to
choose the right storage type per datum (instance vs persistent).

## Proposed solution

1. Move singleton config (`admin`, `verifier`, `token`, `claim`, `epoch`,
   `pool_bal`, `vk_hash`) to **instance** storage and call
   `env.storage().instance().extend_ttl(...)` at the top of every entrypoint.
   Instance storage TTL is extended as a unit, which fits config-sized data.
2. Keep nullifiers in **persistent** storage but `extend_ttl` each one when
   written, with a generous threshold/extension (e.g. extend to the max the
   network allows), and document that claimants must claim within that window —
   or design an explicit archival-safe check.
3. In `groth16_verifier`, `extend_ttl` the `VK` on every `verify` call so an
   actively-used verifier never archives.
4. Add constants for the bump/extend amounts and document them.

## Scope / requirements

- Storage-type migration for config keys (touches all three contracts + tests).
- `extend_ttl` calls on entrypoints and on nullifier writes.
- Tests using `env.ledger().set(...)` / TTL test utilities to advance the ledger
  and assert entries survive.

## Acceptance criteria

- [ ] Config reads/writes extend instance TTL on every entrypoint.
- [ ] `groth16_verifier.verify` extends the VK entry TTL.
- [ ] Nullifier writes extend their own entry TTL.
- [ ] A test advances the ledger past the default TTL and shows
      `execute_payroll` / `claim_payout` / `verify` still work.
- [ ] Extension amounts are named constants, documented in `README.md`.

## Relevant files

- `contracts/payroll/src/lib.rs`
- `contracts/claim/src/lib.rs`
- `contracts/groth16_verifier/src/lib.rs`
- `README.md` ("Storage layout" table)

## Estimated difficulty

**Medium.** Mechanical but broad; the instance/persistent split needs a careful
pass and the TTL tests are a little fiddly.
