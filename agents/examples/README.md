# Apollia OS, agent examples

This directory holds small reference agents that mirror the patterns shown
in the public book. Each example is meant to be read in isolation and used
as a starting point for your own work.

## What is here

| Example | Pattern | Book reference |
|---|---|---|
| `hello/` | Minimal `@on_message` echo agent. | Chapter 5, "Your first agent". |

More examples will land alongside future book chapters. Anything that is
not strictly an example for a book chapter lives outside this directory and
is maintained privately by the project author for real-world work.

## How to install an example locally

```sh
apollia-os agent install agents/examples/hello
apollia-os agent list
apollia-os chat new --agent hello
```

## How to write your own

1. Copy one of the examples.
2. Edit `manifest.toml` to give your agent a name, a version, and a
   description.
3. Rename the Python entry file and update `manifest.toml` to point to it.
4. Implement your handler. The decorators you need (`@agent`, `@on_message`,
   `@skill`, `@orchestrated`) are documented in the book.

## Where to ask questions

For usage questions about the SDK or these examples, open a discussion in
the project's Discussions Q&A. For bugs in an example, open an issue with
the `bug` label.

For real-world worker development (PDF, Excel, custom SaaS integration,
domain-specific agents), see the project website at https://apollia.fr.
