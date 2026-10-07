---
sidebar_position: 11
title: Package and distribute
---

# Package and distribute

A package is what you hand to someone else: one `.apo` file holding agents,
tools and workflows. This guide follows a package from an empty folder to an
installed, signed archive, with the four `package` commands that carry it: `new`,
`build`, `inspect` and `install`.

It assumes Apollia is installed. Writing the content of the package is covered
by [Write a tool](/how-to/write-a-worker) and
[Write a workflow](/how-to/write-a-director).

## What a package is

A package is a folder with an `apollia.toml` manifest at its root. The manifest
declares the agents, which are data, and describes the Python tools and
workflows the folder ships. Built, the folder becomes a `.apo` file: a zip
archive with the manifest at its root, the Python modules and resources at their
relative paths and, when you sign it, a signature that covers every other entry.

Every table and key of the manifest is in the
[package manifest reference](/reference/package-manifest).

## Create the folder

```bash
apollia-os package new invoice-sorter
```

The command writes `invoice-sorter/` under the current folder, or under
`--output-dir`, with an `apollia.toml` that declares one agent and one tool, and
the Python module of that tool. The folder builds as written. Nothing is
overwritten: an existing folder is refused. A package name takes lowercase
letters, digits, `-` and `_`.

## Describe the code

After writing or changing a tool or a workflow, write its description into the
manifest:

```bash
python -m apollia describe invoice-sorter
```

The command rewrites the `[[tools]]` and `[[workflows]]` tables from the
decorators and keeps the rest of the manifest as written. This is the only step
that imports your code, and it runs on your machine: building, inspecting and
previewing a package never run Python.

## Build the archive

```bash
apollia-os package build invoice-sorter
```

The archive is written as `<name>-<version>.apo` in the current folder, or where
`--output` says. Building validates the manifest, and refuses a folder whose
Python modules define tools or workflows that the manifest does not describe. A
refusal names what is wrong: an unknown key with its full path, an identifier
outside `[a-z0-9_-]+`, a version that is not semver.

## Sign it

```bash
apollia-os package build invoice-sorter --sign
```

`--sign` signs the archive with your author key. The key is read from the secret
store under the label `--key` gives, `default` when omitted; it is generated and
stored there the first time. The archive then carries the signature and the
public key that made it.

An unsigned package installs, with a notice. A signature that does not match the
content is refused.

## Inspect before sharing

```bash
apollia-os package inspect invoice-sorter-0.1.0.apo
```

`inspect` shows the manifest, the entries of the archive and the signature
state: `unsigned`, or `signed` with the declared publisher and the fingerprint
of the key. It needs no runtime, so it also serves on a machine where you only
want to read what a file holds.

## Install

```bash
apollia-os package preview invoice-sorter-0.1.0.apo
apollia-os package install invoice-sorter-0.1.0.apo --confirm-deps
```

Both need the runtime. `preview` writes nothing: it reports the agents,
workflows and tools, the accesses each agent asks for, the publisher and the
signature, the connections the package requires and whether they are present,
and the MCP servers it brings. `install` installs the package; `--confirm-deps`
accepts the Python dependencies its `[pip]` table lists, which go into an
environment of the package.

In the desktop application, opening a `.apo` file shows the same preview, and the
package is installed only from there.

## Update

Installing a newer version of an installed package updates it. The overrides a
user set on its agents are kept.

```bash
apollia-os package install invoice-sorter-0.2.0.apo --confirm-changes
```

`--confirm-changes` accepts what the update adds: accesses, required
connections, MCP servers. Read `package preview` first, which lists them. A
version older than the installed one is refused unless `--allow-downgrade` is
given.

## Trust a publisher

A recipient who trusts your key sees your packages under the name they chose
for you:

```bash
apollia-os package trust import --label "Example Accounting" <key>
apollia-os package trust list
```

`<key>` is the public key, as 64 hexadecimal characters or as a file holding
them. `package trust remove <fingerprint>` stops trusting it.

## Manage installed packages

```bash
apollia-os agent package list
apollia-os agent package show invoice-sorter
apollia-os agent package uninstall invoice-sorter --confirm
```

Uninstalling removes the agents of the package with their overrides, its
workflows and tools, its environment, its files and the MCP servers it brought.
The connections it required stay, and the memory and history of its agents are
kept unless `--clear-memory` or `--clear-history` is given.

## Share an agent made in the interface

An agent created in the application, with no code, exports as a package too:

```bash
apollia-os agent export my-agent --sign
```

Its current values become the defaults of the package. No secret, no memory and
no history is exported.

## Distribute

There is no index and no registry: a `.apo` is a file, and you distribute it the
way you distribute any file. The recipient previews it, then installs it.

## Related

- The [package manifest reference](/reference/package-manifest).
- Every `package` subcommand and flag is in the [CLI reference](/reference/cli).
