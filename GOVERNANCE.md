# Governance

Apollia OS is a single-maintainer, source-available project. This page states
how it is run so expectations are clear before you open an issue or a
discussion.

## Maintainer

The project is maintained by one person (see [CODEOWNERS](.github/CODEOWNERS)).
Every change to the public tree is authored or reviewed by the maintainer.
Direction, scope, and release timing are the maintainer's call.

## Contribution model

- **Issues: yes.** Bug reports and feature requests are welcome through the
  [issue templates](.github/ISSUE_TEMPLATE).
- **Discussions: yes.** Usage questions and open-ended ideas belong in
  [Discussions](https://github.com/Apollia-OS/apollia-os/discussions).
- **Pull requests: auto-closed by policy.** Reviewing and maintaining external
  code is a commitment a single maintainer cannot make today. An automated
  workflow closes incoming PRs with a pointer back to issues. This is not a
  personal rejection. See [CONTRIBUTING.md](CONTRIBUTING.md) for the rationale.

## How decisions are made

1. A problem is raised (issue, discussion, or the maintainer's own backlog).
2. If it changes architecture, it is recorded as an Architecture Decision
   Record under [`docs/adr/`](docs/adr/), which is append-only.
3. Implementation happens on the maintainer's side, with credit in the
   [changelog](CHANGELOG.md) where applicable.

The eight non-negotiable principles in [AGENTS.md](AGENTS.md) bound every
decision. A deviation requires an ADR that documents its scope and expiry.

## Quality gates

Because there is one human, quality is enforced by automation rather than by a
review committee: CI runs formatting, Clippy, the full test suite,
`cargo audit`, and `cargo deny` on every change, with cross-platform and
coverage checks on a nightly schedule.

## Security

Report vulnerabilities privately through
[GitHub Security Advisories](https://github.com/Apollia-OS/apollia-os/security/advisories/new)
or, as a fallback, by email to `admin@apollia.fr`. See [SECURITY.md](SECURITY.md)
for the response timeline and scope.

## How this may evolve

This model fits a `v0.1.x` preview. If the project grows a community that
warrants it, the contribution policy and this document will be revisited in the
open. Until then, the model above is deliberate, not a placeholder.
