# Contributing to Apollia OS

Thank you for your interest in Apollia OS. Please read this short page
before opening anything.

Apollia OS is currently a **single-maintainer preview**.
The contribution model reflects that. The short version: **issues yes,
pull requests no**.

## Reporting a bug

If something does not work as documented, please open an issue using the
[bug report template](.github/ISSUE_TEMPLATE/bug_report.md).

Include:

- What you tried (command, configuration, exact UI steps).
- What you expected to happen.
- What actually happened, with logs or screenshots.
- Your platform: OS, Apollia version (`apollia-os --version`), Rust toolchain
  if you built from source.

Good reproduction steps are the single most valuable thing you can give.

## Requesting a feature

Open an issue using the
[feature request template](.github/ISSUE_TEMPLATE/feature_request.md).
Describe the problem you are trying to solve, not just the solution you have
in mind. The maintainer may refine the shape before any work happens.

## Asking a question

Usage questions, configuration help, "how do I do X", and general discussion
belong in
[Discussions Q&A](https://github.com/Apollia-OS/apollia-os/discussions/categories/q-a).
They are not bugs and not feature requests, so please do not open issues
for them.

## Reporting a security vulnerability

Please report security vulnerabilities **privately** through
[GitHub Security Advisories](https://github.com/Apollia-OS/apollia-os/security/advisories/new)
rather than as a public issue. The maintainer will respond and coordinate
disclosure.

## Why pull requests are auto-closed

Apollia OS is maintained by a single person. Reviewing a pull request well
takes time, and accepting an external contribution implies a commitment to
maintain it. Rather than leave PRs in limbo, the project is explicit: an
automated workflow closes incoming PRs with a polite message that points
back to issues.

This is not a personal rejection. If you have a good idea, please open an
issue describing it. If the maintainer agrees and has bandwidth, the
implementation will follow on the maintainer's side, with credit in the
changelog where applicable.

If you need a contribution path that does accept external PRs, several other
agent runtimes welcome them. This project will not, at least for the
v0.1.x line.

## Supporting the project

Code contributions are not accepted, but you can still help Apollia OS survive
and grow. If you want to back the work financially, see the
[Support section of the README](README.md#support-apollia-os) (Patreon, GitHub
Sponsors, Ko-fi). Funding is the most direct way to influence what gets built
next.

## Code of Conduct

All participation in this project, in issues, Discussions, and any other
space tied to the repository, is governed by the
[Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md).
