# Maintainers

This file lists the people who can review and merge pull requests, cut releases,
and handle security reports for MeritPay Contracts.

| Name / handle | Areas | Contact |
|---|---|---|
| _add maintainer_ | contracts, circuits, releases | via GitHub |

## Responsibilities

- Triage new issues within a week; apply labels and link to `issues/*.md` backlog
  entries where relevant.
- Review PRs against `CONTRIBUTING.md` standards. At least one maintainer
  approval is required to merge; two for breaking changes (circuit public
  signals, contract interfaces, trusted setup).
- Coordinate trusted-setup / verification-key rotation with the frontend
  repository when a circuit changes.
- Handle security reports per `SECURITY.md` (acknowledge, triage, fix under
  embargo, coordinate disclosure).
- Keep `CHANGELOG.md` accurate and cut tagged releases.

## Becoming a maintainer

Contributors with a track record of high-quality, reviewed PRs and consistent
participation in issue triage may be nominated by an existing maintainer. Add
the new maintainer to this file in the same PR that grants repository
permissions.

## Decision making

Routine changes: lazy consensus — a PR with one maintainer approval and no
outstanding objections after review may merge.

Contested or breaking changes: discussed on the issue until maintainers agree.
If consensus cannot be reached, the change does not land.
