# Contributing to Apollia OS

Thank you for your interest in Apollia OS. Please read this short page
before opening anything.

Apollia OS is maintained by the Apollia team. The short version of the
contribution model: **issues are welcome, external pull requests are closed**.
[GOVERNANCE.md](GOVERNANCE.md) says who decides and how.

## Reporting a bug

If something does not work as documented, please open an issue using the
[bug report template](https://github.com/Apollia-OS/apollia-os/issues/new?template=bug_report.yml).

Include:

- What you tried (command, configuration, exact UI steps).
- What you expected to happen.
- What actually happened, with logs or screenshots.
- Your platform: OS, Apollia version (`apollia-os --version`), Rust toolchain
  if you built from source.

Good reproduction steps are the single most valuable thing you can give.

A build that fails on your machine goes through the
[build failure template](https://github.com/Apollia-OS/apollia-os/issues/new?template=build_failure.yml),
which asks for the toolchain, the interpreter and the output the build stopped
on.

## Requesting a feature

Open an issue using the
[feature request template](https://github.com/Apollia-OS/apollia-os/issues/new?template=feature_request.yml).
Describe the problem you are trying to solve, not just the solution you have
in mind. The maintainers may refine the shape before any work happens.

## Asking a question

Usage questions, configuration help, "how do I do X", and general discussion
belong in
[Discussions Q&A](https://github.com/Apollia-OS/apollia-os/discussions/categories/q-a).
They are not bugs and not feature requests, so please do not open issues
for them.

## Reporting a security vulnerability

Please report security vulnerabilities **privately** through
[GitHub Security Advisories](https://github.com/Apollia-OS/apollia-os/security/advisories/new)
rather than as a public issue. [SECURITY.md](SECURITY.md) gives the scope and
the response timeline.

## What happens to a pull request

External pull requests are not accepted at this stage. Accepting one implies a
commitment to maintain it, and the project does not make that commitment
today.

The policy is enforced by a workflow rather than left to a case-by-case
decision, so nobody waits on a review that will not come. On a pull request
from outside the project, it posts a comment pointing back to the issue
templates, applies the `wontfix-policy` label, and closes the pull request.

Open an issue instead. Accepted ideas are implemented by the Apollia team,
with credit in the [changelog](CHANGELOG.md) where applicable.

## Build and test

Building from source is the way to reproduce a defect on the current tree or
to confirm a fix. The [README](README.md#quickstart-from-source) takes a clean
clone to a running agent. Beyond that:

- [AGENTS.md](AGENTS.md) lists the build, lint and test commands, scoped and
  workspace-wide, and the rules the code follows.
- The `justfile` at the root holds the project recipes; `just --list` prints
  them.

## Supporting the project

Code contributions are not accepted, but you can still help Apollia OS grow.
If you want to back the work financially, see the
[Support section of the README](README.md#support-apollia-os): Patreon, Ko-fi,
and the [support page](https://apollia.fr/soutenir).

## Code of Conduct

All participation in this project, in issues, Discussions, and any other
space tied to the repository, is governed by the
[Contributor Covenant Code of Conduct](.github/CODE_OF_CONDUCT.md).
