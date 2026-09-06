# `groth16_verifier` silently substitutes zero points on malformed input and does not validate field elements

**Labels:** security, medium, contracts
**Component:** `contracts/groth16_verifier/src/lib.rs`

## Problem / context

The byte readers swallow malformed input by substituting an all-zero buffer:

```rust
fn read_g1(env: &Env, src: &Bytes, offset: u32) -> Bn254G1Affine {
    let slice: BytesN<64> = src
        .slice(offset..offset + 64)
        .try_into()
        .unwrap_or_else(|_| BytesN::from_array(env, &[0u8; 64]));   // <-- silent fallback
    Bn254G1Affine::from_bytes(slice)
}
```

Problems:

- A too-short proof/VK, or a bad offset, yields a **zero G1/G2 point** instead of
  a hard error. `verify` then proceeds with garbage curve points. Callers get a
  `false`/error that doesn't distinguish "valid proof, wrong statement" from
  "you sent me 200 bytes". `proof_bytes.len() != 256` is checked, but VK slices
  and the per-IC reads at `516 + i*64` are not bounds-checked against
  `vk.len()`.
- Public signals are converted with `Bn254Fr::from_bytes(sig_bytes)` with no
  check that the scalar is `< r` (the BN254 scalar field order). Depending on
  host behavior, non-canonical scalars may be accepted, allowing malleability of
  the public inputs (`s` and `s + r` treated as equal).
- No check that `pi_a`, `pi_b`, `pi_c` and the VK points are on-curve / in the
  correct prime-order subgroup. If the `soroban_sdk` BN254 host functions do
  **not** perform subgroup checks internally, this is a soundness hole
  (small-subgroup / cofactor attacks on the pairing check).

## Proposed solution

1. Replace every `unwrap_or_else(|_| zero)` with a real error return
   (`VerifierError::InvalidProofLength` / `InvalidVkLength`).
2. Bounds-check all VK offsets against `vk.len()` before reading (the
   `set_vk` length validation helps, but `verify` should not assume it).
3. Reduce/validate each public signal against the BN254 scalar field order
   before building `Bn254Fr` (reject non-canonical encodings with a new
   `VerifierError::InvalidPublicSignal`).
4. Confirm from the `soroban_sdk` docs whether `bn254` pairing / MSM host
   functions validate subgroup membership. If not, add explicit subgroup checks
   (or a curve-check host call) for all input points. Document the conclusion in
   a code comment either way.
5. Add tests: truncated VK, truncated proof at each segment boundary,
   out-of-range public signal, and (if applicable) an off-subgroup point.

## Scope / requirements

- Error-returning byte readers + bounds checks in `verify` and `set_vk`.
- Scalar canonicalization for public signals.
- Research + document subgroup-check behavior; add checks if needed.
- New negative tests.

## Acceptance criteria

- [ ] No code path in `verify` can proceed with a zero-substituted point; all
      malformed inputs return a typed error.
- [ ] Out-of-range public signals are rejected.
- [ ] A comment cites whether the host does subgroup validation, with explicit
      checks added if it does not.
- [ ] Tests cover truncation at each segment boundary and a non-canonical signal.

## Relevant files

- `contracts/groth16_verifier/src/lib.rs` (`read_g1`/`read_g2`/`read_fr`,
  `verify`, `set_vk`, tests)

## Estimated difficulty

**Medium.** Straightforward hardening; the subgroup-check research needs the
Soroban SDK docs.
