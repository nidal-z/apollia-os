---
title: Inspect a tool
slug: /operator-help/control/inspect-a-tool
sidebar_position: 3
---

# Inspect a tool

> For operators who want to know what a tool actually is before authorising it: what it takes as input, what it gives back, what it requires, and which credentials are stored against it.

## Prerequisites

- The application is open.
- You can recognise the tool in the list (Web search, File write, Bash, and so on).
- To read the contract of a tool, the developer mode is on (**Settings**, section **Developer**).

## The difference with the Permissions screen

Two screens, two questions:

- **Permissions** answers *"which agents may use this tool, and what do they ask first"*: the accesses of each agent, the consents given, their withdrawal. See [Manage tool permissions](manage-tool-permissions.md).
- **Connections**, section **Built-in tools**, answers *"what is this tool"*: whether it is on, its configuration, and its contract. That is this page.

## Read a tool's contract

1. In the sidebar, click **Connections**, then **Apollia, built-in tools** under **Built-in tools**. The page lists the tools shipped with Apollia, grouped by category, each with its switch and its state (**Active**, **Disabled** or **Key required**).

2. With the developer mode on, click the **Contract** button on the tool's card, or right-click the card and choose **View contract**. A panel opens on the right. With the developer mode off, the page shows neither the button nor the technical identifiers.

3. The panel shows, in order:

   - The **display name** and, under it, the technical identifier used everywhere else (the audit trail, the permission rules, a manifest).
   - Two badges: the **kind** of tool (**Native**, **Connection** or **Python tool**) and the **version**, each shown only when the runtime reports it.
   - The **description** the tool declares about itself.
   - **Required permissions**, as badges. This section is only shown when the tool reports permissions: an absent section means "not reported", never "requires nothing".
   - **Input** and **Output**, as an indented list of fields: name, type, whether it is required or optional, its default value and its allowed values when the schema states them. A schema that is not a field list, and any case the reading cannot flatten, falls back to the raw document, one click away behind **Raw JSON** in both sections.
   - **Credentials**, the keys stored for this tool.

4. Close the panel with the ✕ in the header. Nothing is written from this panel, it reads.

A tool that is not registered in the runtime exposes no contract, and the panel says exactly that instead of showing an empty frame.

## See the configured credentials

At the bottom of the same page, a **Credentials** section lists every credential stored on this machine, one row per tool and key: the tool it belongs to and when it was added. The key name is shown with the developer mode on. **Reload** re-reads the list.

**No value, and no fragment of a value, is ever displayed.** The keys stay encrypted on the machine; only their names and their dates travel to the interface. A key is set in the tool's own configuration panel, behind the **Configure** button of its card.

An empty section is the normal state on a fresh installation: it means no tool has been given a key yet.

## Verification

- The contract panel of the **File write** tool shows an input listing the fields the tool expects.
- The technical identifier shown under the panel title is the one you find in the entries of the [Activity log](../observabilite/read-the-audit-trail.md).

## If it does not work

- **There is no Contract button**: the developer mode is off. Turn it on in **Settings**, section **Developer**.
- **The contract panel says the contract is unavailable**: the tool appears in the list but is not registered in the runtime. Nothing is broken on your side, there is simply nothing to describe.
- **The Input section shows raw JSON instead of a field list**: the schema is not a plain field list. The raw document is the whole contract, nothing was hidden.
- **A tool has a key configured but nothing shows in the Credentials section**: click **Reload**. The list is read when the page opens, not on every change made elsewhere.

> **Technical reference:** [Apollia reference](/reference)
