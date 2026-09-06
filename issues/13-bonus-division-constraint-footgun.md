# Bonus calculation `bonus * 100 === baseSalary * bonusRate` is unsatisfiable for many valid salaries

**Labels:** bug, medium, circuits
**Component:** `circuits/payroll_aggregator.circom`, `circuits/claim.circom`

## Problem / context

Both circuits compute the bonus division-free:

```circom
salaryTimesRate <== baseSalary * bonusRate;      // bonusRate ∈ {0, 10, 20, 30}
bonus <-- salaryTimesRate / 100;                 // witness hint (integer division)
bonus * 100 === salaryTimesRate;                 // constraint
```

`bonus <-- salaryTimesRate / 100` computes an **integer** quotient, but the
constraint `bonus * 100 === salaryTimesRate` requires `salaryTimesRate` to be an
**exact multiple of 100**. If `baseSalary * bonusRate` is not divisible by 100,
**no witness satisfies the constraint** and proof generation fails outright.

`bonusRate` is one of `{0, 10, 20, 30}`:

- rate `10` or `30` → needs `baseSalary` divisible by `10`
- rate `20` → needs `baseSalary` divisible by `5`
- rate `0` → always fine

So an employee whose base salary in circuit units is, say, `50003` produces an
un-provable payroll batch — the whole batch of 5 fails because of one salary.
The mock data in `scripts/gen_proof.js` uses round multiples of `5_000_000`, so
the bug is completely hidden in the demo.

The README's unit table (`XLM × 1000` → circuit units, `× 10_000` → stroops)
does not guarantee divisibility by 100 for arbitrary XLM amounts with fractional
values.

## Proposed solution

Pick one:

1. **Define the bonus as floor division and constrain it correctly:**
   introduce a remainder signal `rem`, constrain
   `salaryTimesRate === bonus * 100 + rem` and `rem < 100` (range-checked). The
   payout is then `baseSalary + bonus` with a well-defined rounding rule. Apply
   the **same** rule in `payroll_aggregator` and `claim` so their totals agree
   (cross-reference issue #12's escrow invariant).
2. **Require divisibility explicitly and document it:** add a clear constraint +
   README note that base salaries must be multiples of 100 circuit units, and
   make the frontend round/validate before proof generation. Weaker, but simple.

Option 1 is recommended because it removes a silent failure mode for honest
users.

## Scope / requirements

- Circuit change in both `payroll_aggregator.circom` and `claim.circom`, kept
  bit-identical in bonus semantics.
- Regenerate `build/` artefacts; re-check constraint counts / ptau sizing.
- Tests: prove a batch with a non-divisible salary now succeeds (option 1) or
  fails with a clear documented reason (option 2), and that aggregator total ==
  sum of per-employee claim amounts.
- README "unit system" section updated with the rounding rule.

## Acceptance criteria

- [ ] A payroll batch containing a base salary not divisible by 100 either
      proves successfully with a documented rounding rule, or is rejected with a
      documented, frontend-catchable validation.
- [ ] `payroll_aggregator` and `claim` compute the identical bonus for identical
      inputs (test with shared vectors).
- [ ] `scripts/gen_proof.js` gains at least one employee with a "hard" salary.
- [ ] README documents the rule.

## Relevant files

- `circuits/payroll_aggregator.circom` (steps E–F)
- `circuits/claim.circom`
- `scripts/gen_proof.js`
- `scripts/setup.sh`
- `README.md` ("The unit system")

## Estimated difficulty

**Medium.**
