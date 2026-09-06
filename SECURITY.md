# Security Policy

MeritPay is a zero-knowledge payroll system that custodies funds on-chain. The
circuits are the security boundary of the protocol — the contracts assume the
proofs they verify are sound. We take vulnerability reports seriously.

## Supported versions

This repository is pre-production. The `main` branch is the only supported
target for security fixes. Deployed testnet contracts are experimental and use a
**single-contributor trusted setup** — do not use them with real funds.

| Version | Supported |
|---|---|
| `main` (latest) | ✅ |
| tagged releases | latest tag only |
| testnet deployments | best-effort |

## Reporting a vulnerability

**Do not open a public issue, discussion, or pull request for a security
vulnerability.**

Report privately using **GitHub's private vulnerability reporting**:

1. Go to the repository's **Security** tab → **Report a vulnerability**.
2. Provide the details described below.

If GitHub private reporting is not available to you, open a minimal issue titled
"Security contact request" with no technical detail and a maintainer will reach
out with a private channel.

### What to include

- Affected component: which circuit (`kpi`, `payroll_aggregator`, `claim`,
  `auditor_disclosure`) or contract (`groth16_verifier`, `payroll`, `claim`).
- Commit hash / branch you tested against.
- A description of the issue and its impact (fund loss, proof forgery, soundness
  break, authorization bypass, denial of service, information disclosure).
- Step-by-step reproduction: inputs, commands, expected vs actual behavior. A
  failing test or a witness/proof that should be rejected but isn't is ideal.
- Any suggested remediation.

### What is in scope

- Soundness bugs in the Circom circuits (a prover can satisfy the circuit with
  values that violate the intended statement).
- Proof forgery or malleability against `groth16_verifier` (wire-format parsing,
  missing subgroup/field checks, pairing arrangement).
- Authorization bypasses in `payroll` / `claim` (admin gating, `require_auth`,
  initialization front-running).
- Fund-loss or fund-lock paths (escrow accounting, replay via nullifiers,
  reentrancy, integer decoding of public signals).
- Trusted-setup / VK handling weaknesses that let an attacker swap or forge a
  verification key.
- Vulnerabilities in `scripts/` that could compromise a deployer's keys or push
  a malicious VK.

### What is out of scope

- The known limitation that `setup.sh` performs a single scripted Phase 2
  contribution (documented in `README.md`). We know; a real ceremony is future
  work.
- Findings that require a compromised admin key or a malicious deployer.
- Missing rate-limiting / gas/fee-griefing on permissionless read methods.
- Issues only reproducible against a modified circuit or contract.
- Anything already tracked in [`issues/`](issues/) — but if you can turn a
  tracked concern into a concrete exploit, that is worth reporting.

## Disclosure process

1. **Acknowledgement** within 5 business days.
2. **Triage & severity assessment** within 10 business days, shared with you.
3. We develop and test a fix privately. For circuit soundness bugs this may
   require a new trusted setup and coordinated VK rotation.
4. **Coordinated disclosure**: we agree on a release date with you. Default
   embargo is 90 days from acknowledgement, shorter if a fix ships sooner or the
   issue is being actively exploited.
5. We credit reporters in the release notes and `CHANGELOG.md` unless you prefer
   to remain anonymous.

## Past reviews

An internal audit and its fixes are recorded in
[`SECURITY_AUDIT_REPORT.md`](SECURITY_AUDIT_REPORT.md). It is a historical
snapshot, not a statement that the codebase is currently free of
vulnerabilities.

## Safe harbor

We will not pursue or support legal action against anyone who:

- makes a good-faith effort to comply with this policy,
- avoids privacy violations, data destruction, and service degradation, and
- reports promptly and does not exploit the issue beyond what is needed to
  demonstrate it.

There is currently no paid bug bounty.
