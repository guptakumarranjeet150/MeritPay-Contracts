# Repository hygiene: missing LICENSE, hardcoded contract ID in `upload_vk.js`, doc path inconsistencies

**Labels:** developer-experience, documentation, low
**Component:** repo root, `scripts/upload_vk.js`, `scripts/setup.sh`, `README.md`

## Problem / context

A cluster of small papercuts that hurt a project meant to attract contributors:

1. **No `LICENSE` file.** `README.md` never states a license; `git ls-files`
   shows none. Contributors and integrators have no legal basis to use the code.
   (The exported `build/solidity_ref/*.sol` are GPL-3.0 from snarkjs — worth
   noting so it doesn't get conflated with the project license.)

2. **No `CONTRIBUTING.md` / issue templates.** No guidance on toolchain versions,
   how to run tests, or PR expectations.

3. **`scripts/upload_vk.js` hardcodes a fallback verifier contract ID:**
   ```js
   const VERIFIER_ID = process.argv[3] || process.env.NEXT_PUBLIC_VERIFIER_CONTRACT_ID ||
     'CBT4QOMJFDYVJMJMHLGWSX5GZI4UMTYNLB7MIVMUIDX2MH73OEENUAYK';
   ```
   A stale ID from someone's old deploy. If both the arg and env var are missing
   it silently uploads to a stranger's contract. Should exit with a clear error.

4. **Doc/script path inconsistencies:**
   - `README.md` says `cp .env.contracts ../MeritPay-Frontend/.env.local` and
     also `cp .env.contracts web/.env.local`; `deploy.sh` prints
     `cp .env.contracts web/.env.local`; `setup.sh` syncs artefacts into
     `web/public/circuits/`. The frontend location (`web/` vs
     `../MeritPay-Frontend/`) is never pinned down.
   - `setup.sh` header comment says "8 stages" / "runs in 8 stages" but section
     headers read `1 / 7` … `7 / 7` then an 8th `8 / 8`.
   - `deploy.sh` section headers are inconsistent (`5 / 8`, then `6b / 10`,
     `7 / 10`, `8 / 10`).
   - README "Repository layout" lists `.env.contracts` and `build/` as if
     tracked; both are gitignored — a one-line note would help.
   - `gen_proof.js` banner says "June 2026" / `PAYROLL_EPOCH = 20260621`, README
     dates vary — harmless but sloppy.

5. **`circuits/` include paths** are `include "node_modules/circomlib/..."`,
   which only resolve when `circom` is invoked from the `circuits/` directory or
   with a matching CWD. Prefer `circom -l circuits/node_modules ...` and bare
   `include "circomlib/circuits/poseidon.circom"` for robustness.

## Proposed solution

- Add a `LICENSE` (the maintainer picks; MIT or Apache-2.0 are typical for this
  ecosystem) and a `## License` section in the README noting the snarkjs-derived
  Solidity files are GPL-3.0.
- Add `CONTRIBUTING.md` with toolchain versions + test commands, and a
  `.github/ISSUE_TEMPLATE/`.
- `upload_vk.js`: remove the hardcoded fallback; `console.error` + `exit(1)` when
  no verifier ID is provided. Also validate the VK path exists.
- Fix the stage numbering in both scripts; make the frontend path a single
  documented env var (`FRONTEND_DIR`) used by both scripts and the README.
- Switch circuit includes to `-l` library paths.

## Scope / requirements

- New files: `LICENSE`, `CONTRIBUTING.md`, issue templates.
- Small edits to `upload_vk.js`, `setup.sh`, `deploy.sh`, `README.md`,
  `circuits/*.circom`.

## Acceptance criteria

- [ ] `LICENSE` exists and the README states the license (and the GPL-3.0
      caveat for `build/solidity_ref/`).
- [ ] `upload_vk.js` exits with a clear error instead of using a hardcoded ID.
- [ ] Stage numbers in `setup.sh` / `deploy.sh` are internally consistent.
- [ ] The frontend directory is configured in exactly one place and both scripts
      + README agree.
- [ ] Circuits compile via `circom -l` without depending on CWD.
- [ ] `CONTRIBUTING.md` documents the full local test flow.

## Relevant files

- new: `LICENSE`, `CONTRIBUTING.md`, `.github/ISSUE_TEMPLATE/`
- `scripts/upload_vk.js`, `scripts/setup.sh`, `scripts/deploy.sh`
- `circuits/kpi.circom`, `circuits/claim.circom`,
  `circuits/payroll_aggregator.circom`, `circuits/auditor_disclosure.circom`
- `README.md`

## Estimated difficulty

**Low.** Mostly docs and one-line script fixes; can be split into smaller PRs.
