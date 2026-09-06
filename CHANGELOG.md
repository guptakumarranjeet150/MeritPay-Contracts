# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project aims to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
once it reaches a tagged release. Until then, all changes land under
`[Unreleased]`.

Change types: `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`.

## [Unreleased]

### Added
- Community health files: `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`,
  `LICENSE` (Apache-2.0), `NOTICE`, GitHub issue/PR templates.
- Curated contributor backlog under `issues/`.

### Security
- `claim_payout` now rejects proof signals (`payrollEpoch`, `amount`) whose
  high-order bytes are non-zero instead of silently truncating them to
  `u64` / `i128`.
- Replaced silent zero-fill fallbacks in the `groth16_verifier` byte parser with
  bounds-checked `Result` returns.
- Replaced panic-prone `.unwrap()` calls in the `payroll` and `claim` contracts
  with typed error propagation.

### Fixed
- Circuit constraint ordering for the `baseSalary > 0` check in `claim.circom`
  and `payroll_aggregator.circom`.

<!--
Template for a future release section:

## [0.1.0] - YYYY-MM-DD
### Added
### Changed
### Fixed
### Security
-->
