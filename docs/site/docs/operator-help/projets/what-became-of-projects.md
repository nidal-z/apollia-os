---
title: What became of projects
slug: /operator-help/projects/what-became-of-projects
sidebar_position: 1
---

# What became of projects

> For operators who used the Projects screen before the update, and want to know where their documents, folders and conversations went.

The Projects screen is gone. What a project gathered now lives in the places
every agent reads from: sources, connections, and the conversations of the
Apollia agent. Nothing is injected into a conversation any more: an agent
searches its sources when it needs them.

## What the update did

At its first start after the update, Apollia carries every project over once:

| In a project | Now |
|---|---|
| Its documents | Sources of the Apollia agent, and of the agents the project named, filed under the name of the project. |
| Its instructions | One source per project, searchable, never added to a conversation on their own. |
| Its working folder | A folder connection, granted to no agent until you grant it. |
| Its conversations | Conversations of their agent; a free conversation belongs to the Apollia agent. |
| Its memory | Merged into the memory of the same name. |

## The notice

Once the migration has run, a notice says what became of your projects and what
could not be carried over, for example a document that was no longer where the
project pointed. It shows only once. Each line names the item, its project and
the reason.

When the migration could not finish, the notice says so and Apollia tries again
at the next start. Your former projects stay untouched until a run completes:
the projects database is removed only after everything that could be carried
over was.

## Where to find them

1. **The documents and the instructions**: **Agents**, then the Apollia agent,
   then its sources. They are grouped by project.
2. **The working folders**: **Connections**. Grant a folder to the agents that
   should read it.
3. **The conversations**: **Chat**, with the Apollia agent or with their agent.

## Verification

The notice no longer shows at start, and the documents of a former project are
listed among the sources of the Apollia agent.
