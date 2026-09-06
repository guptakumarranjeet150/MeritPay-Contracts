# `verify_auditor` can never succeed — the auditor (and KPI) verification keys are never deployed

**Labels:** bug, medium, contracts, deployment
**Component:** `contracts/payroll/src/lib.rs`, `scripts/deploy.sh`

## Problem / context

A single `groth16_verifier` instance stores exactly one VK (`symbol_short!("VK")`)
and rejects any call whose public-signal count doesn't match that VK's `n_ic - 1`.

`payroll.verify_auditor` calls the **same** verifier contract that
`execute_payroll` uses:

```rust
let verifier_addr: Address = env.storage().persistent().get(&key_verifier()).unwrap();
let verifier = VerifierClient::new(&env, &verifier_addr);
Ok(verifier.verify(&proof, &public_signals))
```

That verifier holds the **PayrollAggregator** VK (17 public signals,
`n_ic = 18`). The `AuditorDisclosure` circuit has **2** public signals, so
`verify` always returns `Err(SignalCountMismatch)` → `verify_auditor` always
fails. It is effectively dead code.

Related: `scripts/deploy.sh` deploys only two verifier instances (payroll VK,
claim VK). The `kpi` circuit's VK and the `auditor_disclosure` VK are compiled by
`setup.sh` but never uploaded anywhere, and `README.md` implies four circuits are
part of the on-chain system.

## Proposed solution

Pick one direction and make it consistent:

**Option A — support auditor proofs properly:**
- Add an `auditor_verifier` address to `payroll` storage
  (`set_auditor_verifier(admin, addr)` + init param).
- Deploy a third `groth16_verifier` instance in `deploy.sh`, upload
  `build/auditor/auditor_disclosure_vkey.json`.
- Point `verify_auditor` at that address.
- Add a test using a real auditor VK/proof fixture (ties into issue #06).

**Option B — drop it:**
- Remove `verify_auditor` from the contract and its test
  (`test_verify_auditor_passes_through` currently only passes because `MockOk`
  ignores signal counts — it is not testing real behavior).
- Remove auditor/KPI circuits from the "Soroban contracts" narrative in the
  README, or clearly mark them as off-chain / frontend-only.

Also decide whether the `kpi` circuit needs an on-chain verifier at all (it looks
like KPI proofs are consumed off-chain before aggregation — document that).

## Scope / requirements

- Contract change (add auditor verifier config, or remove `verify_auditor`).
- `scripts/deploy.sh` + `.env.contracts` keys updated accordingly.
- `scripts/upload_vk.js` currently hardcodes the payroll VK path in its
  docstring — generalize or document the third upload.
- README corrections.

## Acceptance criteria

- [ ] `verify_auditor` either works against a correctly-scoped verifier (proven
      by a test with a real auditor proof) or is removed.
- [ ] `test_verify_auditor_passes_through` is deleted or replaced with a
      meaningful test.
- [ ] `deploy.sh` and `README.md` agree on exactly how many verifier instances
      exist and which VK each holds.
- [ ] The role of the `kpi` circuit (on-chain vs off-chain) is documented.

## Relevant files

- `contracts/payroll/src/lib.rs` (`verify_auditor`, `initialize`, tests)
- `scripts/deploy.sh`
- `scripts/upload_vk.js`
- `README.md`

## Estimated difficulty

**Medium.**
