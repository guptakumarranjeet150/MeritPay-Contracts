# No end-to-end test that a real snarkjs proof verifies on-chain in `groth16_verifier`

**Labels:** testing, high, contracts
**Component:** `contracts/groth16_verifier/src/lib.rs`, `scripts/gen_proof.js`

## Problem / context

`groth16_verifier` does the most error-prone work in the repo: hand-rolled
byte-parsing of the VK and proof, with specific assumptions about
**endianness**, **G2 limb ordering** (`x_im ‖ x_re ‖ y_im ‖ y_re`), point
negation, and the MSM/pairing arrangement.

Every existing test uses an all-zero dummy VK and never calls the BN254 host
functions with real data:

```rust
fn dummy_vk_bytes(env: &Env, n_public: u32) -> Bytes { /* all zeros */ }
```

`payroll` and `claim` tests use `MockOk` / `MockFail` and skip the verifier
entirely. `scripts/gen_proof.js` verifies proofs **with snarkjs in JS**, not
against the Soroban contract. There is therefore **zero** coverage proving that a
proof produced by `snarkjs` + the wire encoding in `scripts/upload_vk.js`
actually satisfies `Groth16Verifier::verify`. A byte-order bug here would pass
every test in the repo and only surface on testnet.

## Proposed solution

1. Add a committed fixture directory, e.g. `contracts/groth16_verifier/tests/fixtures/`,
   containing for at least one circuit (claim is smallest — 3 public signals):
   - `vk.bin` — the exact bytes `upload_vk.js` would upload
   - `proof.bin` — the 256-byte proof blob in the contract's wire format
   - `public_signals.json` — the `BytesN<32>` signal list
   - `proof_invalid.bin` — the same proof with one byte flipped
2. Add a small generator (`scripts/gen_fixtures.js`, or a `--emit-fixtures` flag
   on `gen_proof.js`) that produces these from `build/` artefacts so they can be
   regenerated after a circuit change. Document the regeneration command.
3. Add an integration test in `groth16_verifier`:
   - `test_verify_accepts_real_proof` — loads the fixtures, `set_vk`, `verify`
     returns `Ok(true)`.
   - `test_verify_rejects_tampered_proof` — `verify` returns `Ok(false)`.
   - `test_verify_rejects_wrong_public_signals` — flip a signal, expect `Ok(false)`.
4. Wire these fixtures into a `payroll` / `claim` test that uses the **real**
   `Groth16Verifier` contract (not `MockOk`) for at least one happy path.

## Scope / requirements

- Fixture files + regeneration script.
- New tests in `groth16_verifier` and at least one in `payroll` and `claim`
  against the real verifier.
- README "Testing" section updated with the fixture regeneration command.
- If `soroban_sdk` BN254 host functions are unavailable in the unit-test env,
  document that and gate the test behind a feature or use the appropriate
  test utility.

## Acceptance criteria

- [ ] `cargo test -p groth16-verifier` exercises `verify` with a real VK + proof
      and asserts `true`.
- [ ] A one-byte-tampered proof asserts `false` (not an error/panic).
- [ ] At least one `payroll` and one `claim` test run against the real
      `Groth16Verifier` contract instead of a mock.
- [ ] Fixtures can be regenerated with a single documented command after a
      circuit change.

## Relevant files

- `contracts/groth16_verifier/src/lib.rs`
- `contracts/payroll/src/lib.rs`, `contracts/claim/src/lib.rs`
- `scripts/gen_proof.js`, `scripts/upload_vk.js`
- `README.md`

## Estimated difficulty

**Medium–high.** The fixture plumbing is the bulk of the work; catches a whole
class of silent wire-format bugs.
