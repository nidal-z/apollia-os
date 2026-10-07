---
title: Read the audit trail
slug: /operator-help/observability/read-the-audit-trail
sidebar_position: 2
---

# Read the audit trail

> For operators who want to find out precisely who did what, and when - typically for an internal control or a post-incident investigation.

The audit trail is the **Activity log**: one journal, signed and chained, that holds every action of the agents and every change of access. The Observability screen no longer carries a separate Audit Trail tab.

## Prerequisites

- At least one action (a tool call, a consent answered, an access granted) has taken place.
- You roughly know the period or the agent to investigate.

## Steps

1. In the sidebar, click **Activity log**. The entries are grouped by day, newest first, each one written as a sentence: when, which agent, what happened, on which target, with which result.

2. Narrow the list with the filters above it:
   - **Agent** - the work of one agent.
   - **Family** - one family of action (read, read personal data, write or delete, send outside, run code).
   - **Nature** - **Actions**, **Access changes** or **Policy**.
   - **Result** - what succeeded, what failed, what was refused.
   - **Period** - **Today**, **Last 7 days**, **Last 30 days** or **The whole journal**.

   **Clear the filters** brings the whole journal back. The line above the list says how many entries are shown and how many filters are active.

3. Click an entry to open its detail in the detail panel. For a tool call, the detail holds the arguments and the output when the run captured them.

4. At the bottom of the list, **Show older entries** extends it.

5. **Verify integrity** recomputes the chain and the signatures, and says whether the journal is intact or where it was altered.

6. **Export** writes the entries that match the filters in force, as **CSV** or as **PDF**. Including the detail adds the parameters of each action, which may hold personal data.

## Export and verify on the command line

The same journal is read from the command line:

```sh
apollia-os audit journal --limit 200     # browse
apollia-os audit export --format csv --output journal.csv
apollia-os audit verify                  # verify the whole chain
apollia-os audit verify <RUN_ID>         # verify one run
apollia-os audit anchor                  # print the head anchor
```

**`verify`** recomputes the hash chain and checks the signatures. With no
argument it walks the entire journal; with a run identifier it is limited to that
run. An entry modified after the fact breaks the chain and shows.

**`anchor`** prints the head anchor of the global chain. Keeping it off the
machine is the only defence against a truncation of the end of the journal by
someone who obtained the signing key. That key is a local file, readable by the
account running Apollia: the exported anchor is therefore the real protection,
not an advanced precaution.

**`export`** writes the entries that pass the filters, newest first, as CSV or
JSON.

Details of these commands in
[Audit and verify a run](/how-to/audit-and-verify).

## Verification

You find in the list the actions you know took place recently, with the correct result. If you filter by agent, the count above the list drops accordingly.

## If it does not work

- **An expected action is missing**: check the filters - an active filter can hide the entry. Widen the **Period**, or click **Show older entries**.
- **The detail of a tool call shows no arguments**: no model response of the run carries that call, as for a tool called by the code of a workflow.
- **The list is empty**: nothing has been journaled yet. Ask an agent for a task that uses a tool.

> **Technical reference:** [Apollia reference](/reference)
