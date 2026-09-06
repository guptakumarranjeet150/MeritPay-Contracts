# MeritPay Contracts — Issue Backlog

Generated from a full-repository review. Each file is a self-contained,
implementation-ready GitHub issue. Suggested priority order:

## Security — do first

| # | Title | Severity | Area |
|---|---|---|---|
| [01](01-verifier-set-vk-missing-access-control.md) | `groth16_verifier.set_vk` has no access control — VK can be swapped by anyone | Critical | contracts |
| [02](02-payroll-execute-does-not-bind-signals.md) | `execute_payroll` never checks `nullifiers` / `total_payroll` against verified signals | High | contracts |
| [03](03-initialize-frontrunning.md) | `initialize` can be front-run; `claim.initialize` has no auth | High | contracts |
| [05](05-circuit-missing-range-checks.md) | Circuits feed unconstrained inputs into `LessThan` — comparator soundness broken | High | circuits |
| [04](04-claim-payout-checks-effects-interactions.md) | `claim_payout` marks the nullifier claimed after the token transfer | Medium | contracts |
| [15](15-verifier-input-validation-hardening.md) | Verifier silently substitutes zero points; no scalar/subgroup validation | Medium | contracts |

## Reliability & correctness

| # | Title | Severity | Area |
|---|---|---|---|
| [09](09-verify-auditor-and-kpi-verifiers-not-wired.md) | `verify_auditor` can never succeed; auditor/KPI VKs never deployed | Medium | contracts / deploy |
| [10](10-vk-hash-is-write-only-dead-feature.md) | `set_verifier_vk_hash` stores a hash nothing reads or checks | Medium | contracts |
| [11](11-persistent-storage-ttl-management.md) | No TTL extension — config and nullifiers can archive | Medium | contracts |
| [12](12-escrow-accounting-and-fund-recovery.md) | No escrow accounting or fund-recovery path; funds can be stranded | Medium | contracts |
| [13](13-bonus-division-constraint-footgun.md) | Bonus constraint unsatisfiable for salaries not divisible by 100 | Medium | circuits |

## Testing & DX

| # | Title | Severity | Area |
|---|---|---|---|
| [06](06-verifier-end-to-end-proof-fixtures.md) | No end-to-end test that a real snarkjs proof verifies on-chain | High | testing |
| [07](07-claim-contract-test-coverage.md) | `claim` contract has only one happy-path test | Medium | testing |
| [14](14-circuit-test-suite.md) | Circuits have no automated test suite | Medium | testing |
| [08](08-add-ci-pipeline.md) | No CI — add GitHub Actions for contracts + circuits | Medium | ci |
| [17](17-config-rotation-and-getters.md) | Contracts can't rotate verifier/admin and expose no config getters | Medium | maintainability |
| [16](16-repo-hygiene-license-scripts.md) | Missing LICENSE, hardcoded contract ID, doc/path inconsistencies | Low | docs / DX |

## Notes

- Issues 01, 02, 03, 05 form the core security set and are somewhat
  interdependent — read them together before starting.
- Issue 06 (real proof fixtures) unblocks meaningful testing for 02, 09, 13.
- `build/`, `.env.contracts`, and `node_modules/` are gitignored; line/section
  references are against the tree at review time.
