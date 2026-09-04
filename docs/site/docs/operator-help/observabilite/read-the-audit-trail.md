---
title: Read the audit trail
slug: /operator-help/observability/read-the-audit-trail
sidebar_position: 2
---

# Read the audit trail

> For operators who want to find out precisely who did what, and when - typically for an internal control or a post-incident investigation.

## Prerequisites

- At least one sensitive action (file write, command, tool call) has been executed by an installed agent.
- You roughly know the period or the agent to investigate.

## Steps

1. In the sidebar, click **Observability**, then the **Audit Trail** tab.

2. At the top of the tab, a **purple callout** recalls what the audit trail is for: internal control, post-incident investigation, compliance, checking an agent's scope of action. The tab records the **tool invocations of an installed agent**. A tool called from a conversation is not written here: the chat path does not write to the trail, and its calls show in the conversation itself.

3. Under the callout, a **Full journal** card gives three totals counted over the whole trail: **Recorded invocations**, **Distinct tools**, **Distinct agents**. These do not move with the filters, and they do not stop at the rows loaded below: they answer "what does the journal hold", where the indicators of the next step answer "what am I looking at right now". If those totals cannot be read, the card shows one line saying so and the table below is unaffected.

4. Just below, **four key indicators** (KPI) update according to the filters: **Entries shown**, **Distinct tools**, **Failures** (red if > 0), **Avg duration**.
   ![Audit Trail tab - purpose banner at the top, 4 KPIs, filters, then the table](/img/operator-help/observabilite-consulter-l-audit-trail-1.png)

5. The table lists every traced tool invocation, newest first. **Five columns**:
   - **Timestamp** (date + local time)
   - **Tool** (technical name in monospace: `bash`, `file_write`, `mcp:notion.search`, and so on)
   - **Agent** (readable name; failing that, the raw identifier if the agent is no longer registered)
   - **Duration** (`850ms`, `2.1s`, or `1m 30s` past the minute, or `-` if not measured)
   - **Status** - an **OK** badge (green, ✓) or **Error** (red, ✕). The status is derived from the exit code *and* from the presence of stderr; an MCP tool with no exit code counts as OK if it finished without an error.

6. Narrow the search with the two selectors above the table:
   - **Tool** - isolates the invocations of one specific tool (the list is built from the loaded entries).
   - **Agent** - isolates the work of a single agent.

7. Click a row to expand its detail. Depending on what was captured, three sections can appear:
   - **Arguments** - JSON sent to the tool, formatted.
   - **stdout** - standard output.
   - **stderr** - error output, shown in red.

   If the invocation produced nothing capturable (read-only MCP tools, tools with no standard I/O…), a *"No details available"* message appears.
   ![expanded row showing the Arguments / stdout / stderr sections](/img/operator-help/observabilite-consulter-l-audit-trail-2.png)

8. At the bottom of the table, the **Load more** button extends the list by 50 more entries.

   > **Export and verification happen on the command line**, not from the
   > interface. See the next section.

## Export and verify on the command line

The interface shows the journal; the command line is what lets you extract it and
prove it has not been modified.

```sh
apollia-os audit list --limit 200        # browse
apollia-os audit stats                   # count
apollia-os audit export --output audit.json --limit 100000
apollia-os audit verify                  # verify the whole chain
apollia-os audit verify <RUN_ID>         # verify one run
apollia-os audit anchor                  # print the head anchor
```

**`verify`** recomputes the hash chain and checks the signatures. With no
argument it walks the entire journal; with a run identifier it is limited to that
run. This is what separates a journal from a plain list: an entry modified after
the fact breaks the chain and shows.

**`anchor`** prints the head anchor of the global chain. Keeping it off the
machine is the only defence against a truncation of the end of the journal by
someone who obtained the signing key. That key is a local file, readable by the
account running Apollia: the exported anchor is therefore the real protection,
not an advanced precaution.

**`export`** writes the journal as JSON. It stops at `--limit`, 10000 by default,
and warns on the error output when it reaches that ceiling.

Details of these commands in
[Audit and verify a run](/how-to/audit-and-verify).

## Verification

You find in the table the actions you know you approved recently, with the correct status (OK in green, Error in red). The KPIs at the top reflect the current selection: if you filter by agent, the **Entries shown** counter drops accordingly.

## If it does not work

- **An expected action is missing**: check the **Tool** and **Agent** filters - an active filter can hide the row. Click **Load more** if the default window (50 entries) does not reach far enough back.
- **A row detail shows nothing**: the invocation probably comes from a tool with no stdout/stderr capture (MCP tool, internal API call). The row stays traced but its inputs and outputs do not.
- **The table is empty**: no installed agent has run a tool yet. A conversation does not feed this table, however many tools it calls. Start an agent on a task that uses tools (bash, file_write, and so on).

> **Technical reference:** [Apollia reference](/reference)
