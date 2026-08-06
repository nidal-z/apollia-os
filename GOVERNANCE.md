# Governance

Apollia OS is a single-maintainer project. This page states
how it is run so expectations are clear before you open an issue or a
discussion.

## Maintainer

The project has a single maintainer (see [CODEOWNERS](.github/CODEOWNERS)).
Every change to the public tree is authored or reviewed by the maintainer.
Direction, scope, and release timing are the maintainer's call.

## Contribution model

- **Issues: yes.** Bug reports and feature requests are welcome through the
  [issue templates](.github/ISSUE_TEMPLATE).
- **Discussions: yes.** Usage questions and open-ended ideas belong in
  [Discussions](https://github.com/Apollia-OS/apollia-os/discussions).
- **Pull requests: auto-closed by policy.** Maintaining external code is a
  commitment the project does not make for the `v0.1.x` line. A workflow
  comments, labels, and closes incoming pull requests, so nobody waits on a
  review that will not come. See [CONTRIBUTING.md](CONTRIBUTING.md).

## How decisions are made

1. A problem is raised (issue, discussion, or the project's own backlog).
2. If it changes architecture, the decisions chapter of the documentation is
   updated in the same change, so the published design never lags the code.
3. Implementation happens on the maintainer's side, with credit in the
   [changelog](CHANGELOG.md) where applicable.

The eight non-negotiable principles in [AGENTS.md](AGENTS.md) bound every
decision. A deviation is documented on the same page as the decision it bends,
with its scope and the condition under which it ends.

## Quality gates

Quality is enforced by automation rather than by a review committee: CI runs
formatting, Clippy, the full test suite, `cargo audit`, and `cargo deny` on
every change, with cross-platform and coverage checks on a nightly schedule.
A rule that is not a gate is a rule that drifts, so the invariants that matter
are checks rather than paragraphs.

## Security

Report vulnerabilities privately through
[GitHub Security Advisories](https://github.com/Apollia-OS/apollia-os/security/advisories/new)
or, as a fallback, by email to `admin@apollia.fr`. See [SECURITY.md](SECURITY.md)
for the response timeline and scope.

## How this may evolve

This model fits a `v0.1.x` preview. If the project grows a community that
warrants it, the contribution policy and this document will be revisited in the
open. Until then, the model above is deliberate, not a placeholder.
