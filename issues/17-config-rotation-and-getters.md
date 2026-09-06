# Contracts cannot rotate the verifier / admin / token and expose no config getters

**Labels:** maintainability, medium, contracts
**Component:** `contracts/payroll/src/lib.rs`, `contracts/claim/src/lib.rs`

## Problem / context

Once initialized, the `payroll` and `claim` contracts hard-lock most of their
configuration:

- **No `set_verifier`.** If a circuit is upgraded (e.g. after fixing issue #05 or
  #13) the VK changes. `groth16_verifier` can accept a new VK (issue #01), but if
  a *new verifier contract* is deployed instead, neither `payroll` nor `claim`
  can be pointed at it — you must redeploy both, re-run `initialize`, re-link,
  re-fund, and migrate escrow. `payroll` only has `set_claim_contract`.
- **No admin rotation / two-step transfer.** `payroll`'s `admin` is set once. A
  lost or compromised admin key cannot be rotated; there is no
  `transfer_admin` + `accept_admin`.
- **No `set_token`.** Reasonable to keep immutable, but should be a deliberate,
  documented choice.
- **No getters.** There is no `get_admin`, `get_verifier`, `get_token`,
  `get_claim_contract` on `payroll`, and nothing on `claim` exposes its
  `payroll` / `verifier` / `token`. Off-chain tooling, the frontend, and audits
  must read raw ledger entries. `get_vk_hash` is also missing (issue #10).

## Proposed solution

1. Add admin-gated setters with events:
   - `payroll.set_verifier(admin, addr)`, `claim`: an `admin` role + `set_verifier`,
     `set_payroll_contract`.
   - `payroll.transfer_admin(admin, new_admin)` / `accept_admin(new_admin)`
     (two-step to avoid setting an unusable address).
2. Add read-only getters on both contracts for every config value.
3. Decide token immutability explicitly; if immutable, document why and add a
   comment.
4. Consider a `paused` flag + `set_paused(admin, bool)` so `execute_payroll` /
   `claim_payout` can be halted during an incident or migration (pairs well with
   issue #12's recovery path).

## Scope / requirements

- New setters/getters + `admin` role on `claim`.
- Two-step admin transfer on `payroll`.
- Events for every mutation.
- `deploy.sh` unchanged for the happy path; document the rotation procedures in
  the README.
- Tests for auth gating and the two-step transfer.

## Acceptance criteria

- [ ] The verifier address on both contracts can be changed by the admin and the
      change is event-logged.
- [ ] `payroll` admin can be rotated via a two-step transfer; a single-step set
      to a wrong address cannot brick the contract.
- [ ] Every stored config value has a public getter.
- [ ] `execute_payroll` / `claim_payout` respect a `paused` flag (if adopted).
- [ ] Tests cover non-admin rejection for every new setter.
- [ ] README documents verifier-swap and admin-rotation procedures.

## Relevant files

- `contracts/payroll/src/lib.rs`
- `contracts/claim/src/lib.rs`
- `README.md`

## Estimated difficulty

**Medium.**
