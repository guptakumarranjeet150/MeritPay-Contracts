# No CI: add a GitHub Actions pipeline for contracts and circuits

**Labels:** ci, developer-experience, medium
**Component:** repository root (`.github/workflows/`)

## Problem / context

There is no `.github/` directory and no CI configuration. Nothing enforces that:

- the workspace builds for `wasm32v1-none`,
- `cargo test` passes for all three contracts,
- code is formatted / lint-clean,
- the circuits still compile and `scripts/gen_proof.js` produces valid proofs,
- committed `test_snapshots/*.json` stay in sync.

Regressions can land on `main` unnoticed. For an open-source project inviting
contributions, a green-check gate is table stakes.

## Proposed solution

Add `.github/workflows/ci.yml` with these jobs:

1. **contracts**
   - `rustup target add wasm32v1-none`
   - `cargo fmt --all --check`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo test --workspace`
   - `cargo build --workspace --target wasm32v1-none --release`
   - cache `~/.cargo` and `target/`
2. **circuits**
   - install `circom` (pinned version) + `cd circuits && npm ci`
   - compile all four circuits (`--r1cs --wasm --sym`)
   - run a lightweight Groth16 setup against the small `pot12` ptau (cache the
     download) and run `scripts/gen_proof.js`, asserting every proof verifies
   - this doubles as regression coverage for issue #05 / #06
3. **shellcheck** on `scripts/*.sh`.

Also add a `rust-toolchain.toml` pinning the toolchain + target so local and CI
builds match, and a status badge to `README.md`.

## Scope / requirements

- `.github/workflows/ci.yml`
- `rust-toolchain.toml`
- Possibly split the heavy circuit job to run only on changes under `circuits/`
  or on a schedule, to keep PR feedback fast.
- Document required tool versions.

## Acceptance criteria

- [ ] PRs run contract build + test + fmt + clippy and block on failure.
- [ ] The circuit job compiles all circuits and runs `gen_proof.js` to a passing
      result.
- [ ] `shellcheck` runs on the deploy/setup scripts.
- [ ] `README.md` shows a CI badge.
- [ ] Caching keeps a no-op PR run under a few minutes for the contracts job.

## Relevant files

- new: `.github/workflows/ci.yml`, `rust-toolchain.toml`
- `scripts/setup.sh`, `scripts/gen_proof.js`, `scripts/deploy.sh`
- `README.md`

## Estimated difficulty

**Medium.** Contract job is standard; the circuit job needs care around ptau
caching and runtime.
