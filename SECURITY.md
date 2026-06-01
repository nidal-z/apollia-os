# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | ✅ Yes     |

## Reporting a Vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Please report security issues privately through
[GitHub Security Advisories](https://github.com/Apollia-OS/apollia-os/security/advisories/new).
If you cannot use GitHub Advisories, email **admin@apollia.fr** instead.

Include in your report:
- A description of the vulnerability and its potential impact
- Steps to reproduce or a proof-of-concept (if available)
- Affected versions

### What to expect

- **Acknowledgement** within 72 hours
- **Status update** within 7 days (confirmed, rejected, or in progress)
- **Patch release** within 30 days for confirmed critical vulnerabilities

We will credit reporters in the release notes unless you prefer to remain anonymous.

## Scope

In scope:
- Remote code execution via agent manifests or tool inputs
- Privilege escalation beyond the declared sandbox profile
- Path traversal in file tools
- Authentication/secret leakage via MCP transport
- StepBudget bypass allowing unbounded agent execution

Out of scope:
- Vulnerabilities in the user's own agent code
- Issues requiring physical access to the machine
- Denial of service against a single local instance
