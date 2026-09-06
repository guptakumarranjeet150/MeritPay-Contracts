# Circuits feed unconstrained inputs into `LessThan` / `LessEqThan`, breaking comparator soundness

**Labels:** security, high, circuits, soundness
**Component:** `circuits/kpi.circom`, `circuits/claim.circom`, `circuits/payroll_aggregator.circom`, `circuits/auditor_disclosure.circom`

## Problem / context

circomlib's `LessThan(n)` / `LessEqThan(n)` are only sound when **both** inputs
are known to be in `[0, 2^n)`. If an input exceeds that range, the comparator
output can be forced to an attacker-chosen value — a well-known Circom footgun.

Several circuits violate this precondition:

### `kpi.circom`
```circom
component lt = LessThan(14);
lt.in[0] <== hoursWorked;      // private, never range-checked
lt.in[1] <== hoursThreshold;   // public input, never range-checked
```
`hoursWorked` and `hoursThreshold` have no range constraint. A malicious prover
can pick `hoursWorked` such that `lt.out` is 0 even when the employee did not
meet the threshold, so `hoursMet` is forged. `employeeId` is documented as
"integer in [1, 5]" but nothing enforces it.

### `claim.circom`
Same pattern: `hoursWorked`, `hoursThreshold` go into `LessThan(14)` unchecked;
`payrollEpoch` and `amount` are unconstrained (only `amount === payout` links it).

### `payroll_aggregator.circom`
`hoursWorked[i]`, `hoursThresholds[i]` into `LessThan(14)` unchecked. Only
`baseSalaries[i]` gets an explicit `< 2^40` bound. `totalPayroll`,
`payrollEpoch`, `employeeIds[i]` unconstrained; employee-id **uniqueness within a
batch** is not enforced.

### `auditor_disclosure.circom`
```circom
ltPayout[i] = LessThan(64);
ltPayout[i].in[1] <== 18446744073709551616;   // 2^64 — NOT < 2^64
```
Passing `2^64` as an input to `LessThan(64)` itself violates the precondition.
`leBudget = LessEqThan(64)` gets `totalPayroll` / `budget` with no prior range
check.

## Proposed solution

For every value that flows into a comparator (or is security-relevant):

1. Add explicit `Num2Bits(k)` / `LessThan` range assertions with documented
   bounds, e.g. `hoursWorked, hoursThreshold ∈ [0, 2^14)`,
   `baseSalary ∈ [1, 2^40)`, `payrollEpoch ∈ [0, 2^40)`,
   `amount ∈ [0, 2^64)`.
2. In `kpi.circom` and `payroll_aggregator.circom`, constrain `employeeId` to
   `[1, MAX_EMPLOYEE_ID]` and enforce pairwise-distinct `employeeIds[i]` in the
   aggregator (or document why duplicates are acceptable).
3. In `auditor_disclosure.circom`, use a bound that is strictly less than the
   comparator modulus (`LessThan(64)` with `in[1] <= 2^64 - 1`, or widen to
   `LessThan(65)`), and range-check `totalPayroll` / `budget`.
4. Add a short "input domains" table to `README.md` under **Circuits** and inline
   comments citing the circomlib precondition.

## Scope / requirements

- Range-constraint additions across all four circuits.
- Employee-id domain + uniqueness handling in `kpi` / `payroll_aggregator`.
- Regenerate `build/` artefacts; note the constraint-count change (may affect the
  pot12 vs pot14 choice in `scripts/setup.sh` — re-verify).
- Negative tests (see the circuit-test-suite issue) proving a forged
  `hoursMet` / over-budget total is now unsatisfiable.

## Acceptance criteria

- [ ] Every `LessThan` / `LessEqThan` input in the four circuits has a proven
      range within the comparator's bit width.
- [ ] `employeeId` domain is enforced in `kpi.circom` and `payroll_aggregator.circom`.
- [ ] A witness with `hoursWorked` outside `[0, 2^14)` is rejected by the circuit.
- [ ] `auditor_disclosure.circom` no longer passes `2^64` into `LessThan(64)`.
- [ ] `scripts/setup.sh` still completes; ptau sizing re-checked against new
      constraint counts.
- [ ] `README.md` documents the accepted input domain for each public signal.

## Relevant files

- `circuits/kpi.circom`
- `circuits/claim.circom`
- `circuits/payroll_aggregator.circom`
- `circuits/auditor_disclosure.circom`
- `scripts/setup.sh` (constraint-count / ptau comments)
- `README.md`

## Estimated difficulty

**Medium–high.** Circom range-checking is mechanical but easy to get subtly
wrong; needs careful review and negative tests. This is the project's actual
security boundary per the README's own "Security notes".
