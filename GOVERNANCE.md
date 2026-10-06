# Governance

Apollia OS is maintained by the Apollia team. This page states how the project
is run, so expectations are clear before you open an issue or a discussion.

## Maintainers

The maintainers are the members of the team named in
[CODEOWNERS](.github/CODEOWNERS). Every change to the public tree is authored
or reviewed by a maintainer. Direction, scope, and release timing are decided
by the maintainers.

## Contribution model

- **Issues: yes.** Bug reports and feature requests are welcome through the
  [issue templates](.github/ISSUE_TEMPLATE).
- **Discussions: yes.** Usage questions and open-ended ideas belong in
  [Discussions](https://github.com/Apollia-OS/apollia-os/discussions).
- **External pull requests: closed by policy.** Maintaining external code is a
  commitment the project does not make at this stage. A workflow comments,
  labels, and closes incoming pull requests, so nobody waits on a review that
  will not come. See [CONTRIBUTING.md](CONTRIBUTING.md).

## How decisions are made

1. A problem is raised, in an issue, in a discussion, or by the team.
2. If it changes architecture, the decisions chapter of the documentation is
   updated in the same change, so the published design never lags the code.
3. Implementation is done by the Apollia team, with credit in the
   [changelog](CHANGELOG.md) where applicable.

The eight non-negotiable principles in
[The 8 principles](docs/site/docs/explanation/the-8-principles.md) bound every
decision. A deviation is documented on the same page as the decision it bends,
with its scope and the condition under which it ends.

## Quality gates

Quality is enforced by automation rather than by a review committee. Every push
to `main` and `develop`, and every pull request, runs the gates declared in
[.github/workflows/ci.yml](.github/workflows/ci.yml): formatting, Clippy, the
test suites, the repository's own guards on prose and capability claims, a
line-coverage floor, and `cargo deny`, which answers the advisory, bans,
licenses and sources questions in one pass. Heavier work runs on a weekly
schedule from [.github/workflows/nightly.yml](.github/workflows/nightly.yml):
cross-platform end-to-end runs, mutation testing, and a full dependency check.

Those two files are the source. This page names families rather than a list,
because a list copied into prose is wrong the day a gate is added or removed.
A rule that is not a gate is a rule that drifts, so the invariants that matter
are checks rather than paragraphs.

## Security

Report vulnerabilities privately through
[GitHub Security Advisories](https://github.com/Apollia-OS/apollia-os/security/advisories/new)
or, as a fallback, by email to `contact@apollia.fr`. See
[SECURITY.md](SECURITY.md) for the response timeline and scope.

## How this may evolve

This model fits the project's current stage. If the project grows a community
that warrants it, the contribution policy and this document will be revisited
in the open. Until then, the model above is deliberate, not a placeholder.
