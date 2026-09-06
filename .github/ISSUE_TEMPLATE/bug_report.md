---
name: Bug report
about: A circuit, contract, or tooling defect (NOT a security vulnerability)
title: "bug: "
labels: bug
assignees: ''
---

<!--
STOP: If this is a security vulnerability — proof forgery, circuit soundness
break, authorization bypass, fund loss/lock — do NOT file it here.
Follow SECURITY.md for private disclosure.
-->

## Component

- [ ] Circuit: `kpi`
- [ ] Circuit: `payroll_aggregator`
- [ ] Circuit: `claim`
- [ ] Circuit: `auditor_disclosure`
- [ ] Contract: `groth16_verifier`
- [ ] Contract: `payroll`
- [ ] Contract: `claim`
- [ ] Script: `setup.sh` / `deploy.sh` / `gen_proof.js` / `upload_vk.js`
- [ ] Docs / other

## Description

<!-- What is wrong? -->

## Steps to reproduce

1.
2.
3.

```
# exact commands
```

## Expected behavior

## Actual behavior

<!-- Paste the full output / error / failing assertion. -->

```
```

## Environment

- Commit / branch:
- OS:
- Rust: `rustc --version`
- Node: `node --version`
- circom: `circom --version`
- Stellar CLI: `stellar --version`

## Additional context

<!-- Related issues, a link to an issues/*.md file, a minimal failing test, etc. -->
