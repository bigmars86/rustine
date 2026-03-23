# Security Policy

## Supported Versions

| Version | Supported          |
|---------|--------------------|
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

If you discover a security vulnerability in Rustine, please report it
responsibly.

**Do NOT open a public GitHub issue.**

Instead, use [GitHub's private vulnerability reporting](https://github.com/bigmars86/rustine/security/advisories/new)
to submit your report.  This keeps the details confidential until a fix
is available.

### What to include

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

### Response timeline

- **Acknowledgement:** within 48 hours
- **Initial assessment:** within 1 week
- **Fix and disclosure:** coordinated with reporter, typically within 30 days

## Dependency Auditing

This project uses:
- [`cargo deny`](https://github.com/EmbarkStudios/cargo-deny) for license
  and advisory checks
- [`cargo audit`](https://github.com/RustSec/rustsec) via CI for known
  vulnerabilities in dependencies
- [Dependabot](.github/dependabot.yml) for automated dependency updates
