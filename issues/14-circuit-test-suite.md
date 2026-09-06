# Circuits have no automated test suite (only a demo proof script)

**Labels:** testing, medium, circuits
**Component:** `circuits/`, `scripts/gen_proof.js`

## Problem / context

`scripts/gen_proof.js` generates proofs from one fixed set of "everything is
valid" mock employees and checks they verify. It is a demo, not a test suite:

- No **negative** tests — e.g. proving that a witness claiming `hoursMet = 1`
  when `hoursWorked < hoursThreshold` is rejected, or that `totalPayroll` not
  equal to the sum is rejected, or that a wrong `nullifier` fails
  `nullifier === nullHash.out`.
- No tests for the `claim` circuit or edge cases (zero bonus, max 30% bonus,
  `salesFlag` non-binary rejection).
- No tests pinning the **public signal ordering** the contracts depend on
  (KPI: `[kpiCommitment, hoursMet, salesMet, employeeId, hoursThreshold]`;
  claim: `[nullifier, payrollEpoch, amount]`; aggregator: 17 signals). A circuit
  edit that reorders outputs would silently break `claim.claim_payout`'s
  `public_signals[0/1/2]` indexing.
- `gen_proof.js` has a `computeNullifier` helper that is defined but unused, and
  a fragile circomlibjs fallback that sets nullifiers to `0n`.

## Proposed solution

Add `circuits/test/` using `circom_tester` (`wasm_tester` / `c_tester`) + mocha:

1. **Witness / constraint tests** per circuit:
   - valid input → `calculateWitness` + `checkConstraints` pass, and public
     outputs equal expected values.
   - each `=== ` constraint has a negative case that throws (forged `hoursMet`,
     wrong `nullifier`, `salesFlag = 2`, `totalPayroll` off by one,
     `amount !== payout`, non-invertible `baseSalary = 0`).
2. **Public-signal-order tests**: assert the index of each named signal in the
   witness / `publicSignals` array, so contract assumptions are guarded.
3. Wire `npm test` in `circuits/package.json` and into CI (issue #08).
4. Clean up `gen_proof.js`: remove dead `computeNullifier`, make the nullifier
   computation non-optional (fail loudly if circomlibjs is missing).

## Scope / requirements

- `circuits/package.json`: add `circom_tester`, `mocha`, `chai`, `test` script.
- `circuits/test/*.test.js` for all four circuits.
- Small compilation fixture step (or reuse `build/`).
- `gen_proof.js` cleanup.

## Acceptance criteria

- [ ] `cd circuits && npm test` compiles the circuits and runs positive +
      negative assertions for kpi, claim, payroll_aggregator, auditor_disclosure.
- [ ] At least one negative test per `===` constraint in each circuit.
- [ ] A test fails if the public-signal order of any circuit changes.
- [ ] `gen_proof.js` no longer contains unused helpers or a silent nullifier
      fallback.
- [ ] CI runs the suite.

## Relevant files

- `circuits/*.circom`
- `circuits/package.json`
- new: `circuits/test/`
- `scripts/gen_proof.js`

## Estimated difficulty

**Medium.**
