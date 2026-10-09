---
sidebar_position: 3
title: 3. Context and scope
format: mdx
---

# 3. Context and scope

This is the C4 system context: Apollia OS as one box, and everything outside it
that it talks to. It sets the boundary before the next pages open the box.

## System context

```mermaid
flowchart TB
    subgraph external_people [People]
        operator["Operator / admin<br/>runs and supervises agents"]
        author["Package author<br/>declares agents, writes tools and workflows"]
    end

    subgraph external_systems [External systems]
        host["Host product<br/>drives Apollia over its API"]
        mcp["External MCP servers<br/>expose host data and tools"]
        cloud["Remote model servers<br/>OpenAI-compatible endpoint, Ollama"]
        saas["Google / Microsoft SaaS<br/>Gmail, Calendar, Drive, Outlook"]
    end

    apollia(["Apollia OS<br/>sovereign agent runtime"])

    localmodel[("Local model<br/>llama-server, on this machine")]
    localdb[("Local SQLite<br/>memory, audit, config")]

    operator -->|CLI, desktop app| apollia
    author -->|builds and installs packages| apollia
    host -->|drives via HTTP API + host SDK| apollia
    apollia -->|MCP client, reads tools| mcp
    apollia -->|calls when configured| cloud
    apollia -->|OAuth, scoped| saas
    apollia -->|supervised sidecar, loopback HTTP| localmodel
    apollia -->|persists| localdb

    classDef sys fill:#1f6feb,stroke:#0b3d91,color:#fff;
    class apollia sys;
```

## Actors and neighbours

| Neighbour | Relationship | Direction |
|---|---|---|
| **Operator / admin** | Runs, supervises, and approves. Uses the [CLI](/reference/cli) and the desktop operator app. | Drives Apollia |
| **Package author** | Declares agents in a manifest and writes typed Python tools and workflows against the [SDK contract](/reference/sdk), then builds the package and installs it. | Builds for Apollia |
| **Host product** | Embeds and drives the runtime through the stable [HTTP API](/reference/api/apollia-os-runtime-api) and generated host SDKs. In the federation pattern the host is often Apollia's client and the other way around at once. | Bidirectional |
| **External MCP servers** | Apollia is an MCP client: it discovers and calls their tools over stdio, streamable HTTP, or SSE. It can also expose a limited inbound MCP server. | Apollia calls out (mostly) |
| **Remote model servers** | Optional. A remote model is reached through one generic connection to an OpenAI-compatible server at the address the user designates (a self-hosted server or a gateway), or through Ollama. Local stays the default. | Apollia calls out |
| **Google / Microsoft** | Native connectors act on mail, calendar, and files through OAuth, on scoped permissions. | Apollia calls out |
| **Local model and database** | The model files, the inference engine the daemon supervises and the SQLite store all live on the same machine. Nothing here is remote. | In-boundary |

## What is inside the boundary

Everything that does the agent work: reasoning and planning, the tool calls,
memory, governance (permissions, audit, budgets), local inference, and the
surfaces (API, CLI, desktop). The next page,
[Solution strategy](/architecture/solution-strategy), states the structural
decisions that shape the inside; [Building block view](/architecture/building-blocks)
opens it into its parts.

## What is deliberately outside

The host's own data store stays on the host side and is read through MCP tools,
never copied wholesale into the runtime. Remote inference is opt-in, not a
dependency. There is no Apollia-operated cloud service in the loop: the runtime
is the product, running on the adopter's machine.
