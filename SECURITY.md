# Security Policy

## Alpha Status

Neuro is an **alpha-stage compiler**. It is not yet suitable for production use, and its security posture reflects that. Security-impacting bugs are taken seriously regardless of maturity level.

## Supported Versions

Only the latest release on the `main` branch receives security fixes. No backports are made to older versions during the alpha phase.

## Security Surface

Neuro's security concerns fall into three categories:

**Compiler integrity**: malformed or adversarial `.nr` source files that cause the compiler to crash, panic, read out-of-bounds memory, or exhibit undefined behavior during parsing or analysis.

**Generated code safety**: bugs in the LLVM or MLIR backends that produce incorrect or unsafe native code (e.g., uninitialized memory reads, incorrect pointer arithmetic in generated IR).

**Dependency vulnerabilities**: CVEs in third-party crates (`inkwell`, `melior`, `logos`, etc.) that affect the compiler at build or runtime.

Out of scope for security reports: compiler error messages, lint false positives, missing language features, or performance issues that do not have a security impact.

## Reporting a Vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Report through [GitHub's private security advisory form](https://github.com/PanzerPeter/Neuro/security/advisories/new) with a description, reproduction steps, and an impact assessment. The report stays visible only to the maintainers until a fix is coordinated and released.

## Response Commitment

Given the current single-maintainer alpha stage, the following timelines are best-effort:

| Milestone | Target |
|-----------|--------|
| Acknowledgement | Within 7 days |
| Initial triage | Within 14 days |
| Fix or mitigation | Within 45 days for high-severity issues |
| Public disclosure | After fix is released, or 90 days from report |

If a fix requires significant architectural work, the maintainer will communicate a revised timeline within the initial triage window.

## Auditing Dependencies

CI runs OSV-Scanner over the dependency graph. To audit it locally:

```bash
cargo install cargo-audit
cargo audit
```

## Disclosure Policy

Neuro follows **coordinated disclosure**: fixes are developed privately, released, and then the advisory is published. Credit is given to reporters unless they prefer to remain anonymous.
