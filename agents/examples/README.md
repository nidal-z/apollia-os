# Apollia OS, package examples

Two small packages, each meant to be read in one sitting. A package is a
folder with an `apollia.toml` at its root; it holds any combination of agents,
Python tools and workflows.

## What is here

| Example | What it shows |
|---|---|
| `hello/` | An agent declared in `apollia.toml`, granted one Python tool of its own package. |
| `word-count/` | A package with no agent: one Python tool, granted to an agent you created in the application. |

An agent is data. Its mission, its examples and what it may reach are written
in `apollia.toml`, and no Python stands for it. The Python of a package brings
tools (`async` functions under `@tool`) and workflows (`@workflow`).

## Build and read an archive

Building validates `apollia.toml` and writes a `.apo` archive. No Python runs.

```sh
apollia-os package build agents/examples/hello
apollia-os package inspect --json hello-0.1.0.apo
```

`inspect` prints the manifest, the entries of the archive and its signature
state. The archive is written in the current folder unless `--output` names
another place.

## Install and run `hello`

Installing needs the runtime (`apollia-os start`).

```sh
apollia-os package preview hello-0.1.0.apo
apollia-os package install hello-0.1.0.apo
```

`preview` shows what the package contains and asks for, before anything is
written. Then talk to the agent:

```sh
apollia-os run hello "Say hello to Ada."
```

The agent `hello` is granted `pkg:hello/greet` by its package: it calls the
tool and answers with the greeting.

## Grant the tool of `word-count` to your own agent

`word-count` declares no agent. Once installed, its tool is listed with the
others under the name `pkg:word-count/count_words`, and any agent may be
granted it.

```sh
apollia-os package build agents/examples/word-count
apollia-os package install word-count-0.1.0.apo
```

Create an agent in the application, on the Agents screen, then open its sheet
and grant it the tool `count_words`. From a terminal, the same grant is one
line, where `notes` stands for the identifier of your agent, as
`apollia-os agent list --definitions` shows it. `access` takes the whole list,
so repeat the accesses the agent already holds:

```sh
apollia-os agent set notes access pkg:word-count/count_words
apollia-os run notes "How many words are in this sentence?"
```

## After a change to the Python

The `[[tools]]` and `[[workflows]]` entries of `apollia.toml` are written from
the code, so that reading the manifest never runs it. After changing a
decorated function, rewrite them, then build again:

```sh
python -m apollia describe agents/examples/hello
```

`package build` refuses a folder whose Python defines a tool that the manifest
does not describe.

## Write your own

Start from the folder that `apollia-os package new`, followed by the name of
your package, writes in the current folder, not from a copy of an example. It
builds as written.

The authority on what the runtime hands a tool is the type contract in
`sdk/apollia/types.py` and `sdk/apollia/context/`. Read it instead of inferring
from an example: an example shows shape, the types say what is guaranteed.

## Questions

Usage questions belong in Discussions Q&A. A defect in an example is an issue.
