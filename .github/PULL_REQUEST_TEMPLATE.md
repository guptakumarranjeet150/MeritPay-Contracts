<!--
Thanks for contributing to MeritPay Contracts!
Please read CONTRIBUTING.md first. Keep this PR scoped to a single issue.
-->

## Summary

<!-- What does this change do, and why? 2–4 sentences. -->

## Linked issue

<!-- e.g. Closes #123, or "Implements issues/07-claim-contract-test-coverage.md" -->
Closes #

## Type of change

- [ ] `fix` — bug fix (non-breaking)
- [ ] `feat` — new functionality (non-breaking)
- [ ] `security` — hardening / vulnerability fix
- [ ] `test` — test coverage only
- [ ] `docs` — documentation only
- [ ] `refactor` / `chore` / `ci`
- [ ] **Breaking change** (circuit public signals, contract interface, or trusted setup)

## What changed

<!-- Bullet the concrete changes: files, functions, circuits, storage keys. -->

-

## Test plan

<!-- Exact commands and their result. All must pass locally. -->

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets` — clean
- [ ] `cargo test --workspace` — passing
- [ ] `cargo build --target wasm32v1-none --release` — builds
- [ ] Circuit smoke test (`node scripts/gen_proof.js`) — if circuits touched
- [ ] Testnet `./scripts/deploy.sh` run — if deployment/wiring touched
- [ ] New/updated tests cover the change (describe below)

<!-- Paste relevant output. -->

## Acceptance criteria

<!-- Copy the checklist from the linked issue and tick each item. -->

- [ ]

## Contract / circuit impact

- [ ] No change to circuit public signals (count, order, meaning)
- [ ] No change to a contract's public interface
- [ ] No change requiring a new trusted setup / VK upload
- [ ] Storage-layout table in `README.md` updated (if new storage keys)
- [ ] `contracts/*/test_snapshots/` regenerated and committed (if interface changed)
- [ ] `scripts/deploy.sh` updated (if deployment changed)

If any box above is unchecked, explain here and confirm a maintainer + the
frontend repo are aware:

## Docs

- [ ] `README.md` updated
- [ ] `CHANGELOG.md` updated under `## [Unreleased]`

## Checklist

- [ ] I read `CONTRIBUTING.md`
- [ ] Commits follow Conventional Commits and contain no agent/tool attribution
- [ ] This PR is scoped to one issue
- [ ] For a vulnerability, I used the private process in `SECURITY.md` instead of this PR
