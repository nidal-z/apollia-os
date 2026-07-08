---
sidebar_position: 0
title: Architecture
slug: /architecture
---

# Architecture

This is the public system cartography of Apollia OS, written as an
[arc42](https://arc42.org) structure with a [C4](https://c4model.com) model. It
is the map a contributor, an integrator, or a technical evaluator reads to
understand how the runtime is built and why.

## What this section is, and is not

It describes the shape of the system: its goals, its constraints, its parts,
how they fit, and where the debt is. It does not repeat the reference material.
Whenever a fact belongs to the API, the CLI, or the SDK, this section links to
the generated reference rather than restating it, so there is one source per
fact:

- [HTTP API reference](/reference/api/apollia-os-runtime-api) for the host driving contract.
- [CLI reference](/reference/cli) for the command surface.
- [SDK / `ctx` contract](/reference/sdk) for what an agent author calls.
- [Configuration](/reference/configuration) and the
  [native tool catalog](/reference/native-tools).

Concepts that deserve their own narrative live under
[Explanation](/explanation), for example
[the accountability model](/explanation/accountability-model). This section
links to them instead of duplicating them.

## Honesty policy

Every claim here is anchored to what the code actually does, not to what older
design notes hoped it would do. Where a capability is partial or absent, it is
stated plainly in [Risks and technical debt](/architecture/risks-and-technical-debt).
A cartography that hides its gaps is not a map, it is a brochure.

## Reading order

1. [Introduction and goals](/architecture/introduction-and-goals)
2. [Constraints](/architecture/constraints)
3. [Context and scope](/architecture/context-and-scope)
4. [Solution strategy](/architecture/solution-strategy)
5. [Building block view](/architecture/building-blocks)
6. [Runtime view](/architecture/runtime-view)
7. [Cross-cutting concepts](/architecture/crosscutting-concepts)
8. [Architecture decisions](/architecture/decisions)
9. [Risks and technical debt](/architecture/risks-and-technical-debt)
10. [Glossary](/architecture/glossary)
