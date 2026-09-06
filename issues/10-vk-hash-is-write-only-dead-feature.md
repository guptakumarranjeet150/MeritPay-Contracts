# `payroll.set_verifier_vk_hash` stores a hash that nothing ever reads or checks

**Labels:** bug, medium, contracts
**Component:** `contracts/payroll/src/lib.rs`

## Problem / context

`set_verifier_vk_hash(admin, vk_hash)` writes `vk_hash` to persistent storage and
emits an event. Per its doc comment the intent is:

> Stores a SHA-256 digest of the expected VK so the frontend can verify the
> on-chain VK has not been swapped without fetching all VK bytes.

But:

- There is **no getter** (`get_vk_hash` does not exist), so the frontend can't
  read it back except by scraping events.
- There is **no on-chain check** — `execute_payroll` never compares the stored
  hash against the actual VK held by the `groth16_verifier` contract.
- The `groth16_verifier` has no "return a digest of my VK" method either.

So the feature provides no actual protection against the VK-swap attack described
in issue #01. It is write-only state plus an event.

## Proposed solution

Make the integrity check real:

1. Add `Groth16Verifier::vk_hash() -> BytesN<32>` that returns
   `env.crypto().sha256(&stored.raw)` (or keccak — pick one and document).
2. Add `PayrollContract::get_vk_hash() -> Option<BytesN<32>>`.
3. In `execute_payroll` (and optionally `verify_auditor`), if a `vk_hash` is
   configured, cross-call `verifier.vk_hash()` and reject with a new
   `PayrollError::VkMismatch` if it differs. This turns a silent VK swap into a
   hard failure rather than a fund-draining exploit.
4. Do the same for `claim` (`claim_payout` vs its verifier).
5. Document the operator workflow: after `upload_vk.js`, compute the digest and
   call `set_verifier_vk_hash`; `deploy.sh` should do this automatically.

Note: issue #01 (auth on `set_vk`) is the primary fix; this issue hardens the
detection path and removes dead code either way.

## Scope / requirements

- New `vk_hash()` on the verifier, `get_vk_hash()` on payroll (and claim).
- Optional enforcement in `execute_payroll` / `claim_payout` gated on a
  configured hash.
- `deploy.sh` computes and sets the hash after VK upload.
- Tests.

## Acceptance criteria

- [ ] The stored VK hash is readable via a contract getter.
- [ ] With enforcement enabled, swapping the verifier VK causes `execute_payroll`
      to fail with `VkMismatch` (test with a real or mock verifier that returns a
      different digest).
- [ ] With no hash configured, behavior is unchanged (backwards compatible).
- [ ] `deploy.sh` sets the hash automatically and the README documents it.

## Relevant files

- `contracts/payroll/src/lib.rs` (`set_verifier_vk_hash`, `execute_payroll`)
- `contracts/groth16_verifier/src/lib.rs`
- `contracts/claim/src/lib.rs`
- `scripts/deploy.sh`
- `README.md`

## Estimated difficulty

**Medium.**
