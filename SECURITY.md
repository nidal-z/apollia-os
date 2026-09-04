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

## Verifying release artifacts

Every release asset is signed and carries build provenance. Artifact signing is
keyless (Sigstore OIDC): there is no long-lived private key, and the signing
identity is the release workflow itself. One signature is not keyless, and is
described at the end of this section: the OpenPGP signature over `SHA256SUMS`,
whose public key is `apollia-release-key.asc` at the root of this repository.

The CLI archive is named after its preset, and its extension follows the
platform: `apollia-os-<preset>.tar.gz` on macOS and Linux,
`apollia-os-<preset>.zip` on Windows, which packages its archive with
`Compress-Archive`. The authoritative list of published names is
`packaging/artifacts.json`; the commands below use one real name of each shape.

Each artifact ships with a Sigstore bundle (`.cosign.bundle`) carrying its
signature and the signing certificate together. Verify a download with cosign:

```sh
cosign verify-blob \
  --bundle apollia-os-linux-x86-cpu.tar.gz.cosign.bundle \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  --certificate-identity-regexp '^https://github.com/Apollia-OS/apollia-os/' \
  apollia-os-linux-x86-cpu.tar.gz
```

```powershell
cosign verify-blob `
  --bundle apollia-os-windows-x86-cpu.zip.cosign.bundle `
  --certificate-oidc-issuer https://token.actions.githubusercontent.com `
  --certificate-identity-regexp '^https://github.com/Apollia-OS/apollia-os/' `
  apollia-os-windows-x86-cpu.zip
```

### The checksum signature

`SHA256SUMS` covers every published file, and the release signs it with an
OpenPGP key. The public half is committed at the root of this repository as
`apollia-release-key.asc`, so it travels with the source rather than with the
download it authenticates.

```sh
gpg --import apollia-release-key.asc
gpg --verify SHA256SUMS.asc SHA256SUMS
shasum -a 256 -c SHA256SUMS
```

The fingerprint to expect:

```
578236310A23A3DE368737CD8F38159B9DA5C452   Apollia OS <admin@apollia.fr>
```

`gpg` reports the key as untrusted until you certify it, which is expected and
says only that no chain of trust reaches it from your keyring. What matters is
`Good signature` and the fingerprint above.

Each release also carries a SLSA build-provenance attestation. Verify that an
artifact was built by this repository's release workflow with the GitHub CLI,
naming the file you downloaded:

```sh
gh attestation verify apollia-os-linux-x86-cpu.tar.gz --repo Apollia-OS/apollia-os
gh attestation verify apollia-os-windows-x86-cpu.zip --repo Apollia-OS/apollia-os
```

A CycloneDX and an SPDX SBOM (`.cdx.json`, `.spdx.json`) are published next to
each artifact, listing the Rust crates and the embedded Python runtime that
ship inside the bundle. The legacy per-artifact `SHA256` checksums and, when
present, the native OS code signatures (Apple notarization, Windows
Authenticode, Linux GPG on `SHA256SUMS`) remain available and unchanged.
