# Getting help with Apollia OS

Apollia OS is maintained by the Apollia team. There is no support contract and
no response-time commitment for questions and issues; the one exception is a
security report, which follows the timeline in [SECURITY.md](../SECURITY.md).
What follows is the route that gets you an answer fastest, in the order to try
it.

## 1. Read the documentation

The documentation site covers installation, configuration, the native tools,
the SDK contract and the operator help pages: <https://docs.apollia.fr>. The
project site is <https://apollia.fr>. The help corpus also ships inside the
binary, and the built-in companion agent answers from it without leaving your
machine.

## 2. Run the diagnostic

```sh
apollia-os doctor --json
```

Eight checks in one command: home directory, configuration, governance
database, agents database, models directory, Python, sandbox posture, runtime
socket. Most installation problems name themselves here, and its output is the
first thing any report should carry.

## 3. Ask a usage question

Questions about how to use the product, how to configure it, or whether
something is supposed to work belong in Discussions, under Q&A, rather than in
an issue.

## 4. Open an issue

Three forms exist, and each one asks for what the maintainers would otherwise
have to request:

- **Bug report**, for something that does not behave as documented.
- **Build failure**, for a build from source that does not complete.
- **Feature request**, for a capability that does not exist yet.

Read [CONTRIBUTING.md](../CONTRIBUTING.md) first: it states what the project
accepts, and what happens to a pull request.

## 5. Report a vulnerability

Never in a public issue. Use GitHub Security Advisories, as described in
[SECURITY.md](../SECURITY.md): acknowledgement within 72 hours, a status update
within 7 days.
