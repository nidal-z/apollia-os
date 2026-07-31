# Review and clean up memory

> For operators who want to see what their AI has retained, and delete what should no longer be there.

## Prerequisites

- At least one agent that has already held a conversation or run a task.

## Where to find what

Two distinct places, two distinct uses:

- **Settings → Profile** - your user profile (first name, role, sector, agent supervision, sovereignty, and so on). This is what all your agents know about you. See [My profile](gerer-mon-profil.md).
- **Memory** (from the sidebar) - the explorer of **memory namespaces** per agent and per project. This is what each agent retains for itself (conversation episodes, semantic facts, learned procedures).

This page covers the second one: browsing the memory namespaces and deleting entries in them.

## Steps

1. In the sidebar, click **Memory**. The page shows a two-column layout: the namespace sidebar on the left, the central panel with the list of entries for the selected namespace.

   ![Memory page: the namespace list on the left, the type filters and the search in the middle, and the entries of the user profile memory](/img/operator-help/en/memoire-consulter-et-nettoyer-la-memoire-1.png)

2. **Left sidebar** - the list of **namespaces** (one namespace = one isolated memory space). Each namespace is classified automatically:
   - **Profile**: your shared user profile (`__user__`). Read-only from this page, editing happens in **Settings → Profile**. A banner reminds you of that when you select `__user__`.
   - **Agents**: namespaces of installed agents (one per agent, for example `veille-ia`, `email-triage`).
   - **Projects**: namespaces scoped to a project (format `{project_id}:{ns}`).
   - **Other**: legacy namespaces or namespaces of uninstalled agents.

   A **segmented control** at the top filters the list by category. A **Filter…** field lets you find a namespace by name.

3. **Central panel** - the list of entries for the selected namespace, with:
   - A **segmented control** per entry type: **All / Episodic / Semantic / Procedural**.
   - A full-text **search bar** (FTS5) that queries the content.
   - A **breadcrumb** under the filter that recalls the current namespace.
   - Each row shows the type icon, the key, a preview of the value, and the relative date.

4. **Click an entry** to open the **detail panel** on the right. It shows the full value (with automatic JSON pretty-print when applicable), all the metadata (type, namespace, ID, dates, BM25 score in search mode), and exposes two actions: **Copy** the value and **Delete** the entry.

   ![Detail panel of a memory entry, with its value, its metadata and the Copy and Delete actions](/img/operator-help/en/memoire-consulter-et-nettoyer-la-memoire-2.png)

5. To **search**, type a few keywords in the **search bar** at the top of the central panel. Matching entries are shown sorted by relevance (BM25 score), and the breadcrumb reads "*N results*".

6. To **delete a specific entry**, two equivalent options:
   - Hover the row, click the **⋯** menu at the end of the row, choose **Delete**, then confirm on the **Confirm** button that appears.
   - Or open the detail panel (click the row) and use the **Delete** button at the bottom.

   The entry disappears immediately and will not come back.

## Verification

The deleted entry no longer appears in the list, even after reloading the page. A keyword search on that entry no longer returns a result. The namespace counter in the sidebar and the type counter in the segmented control are decremented.

## If it does not work

- **The page is empty**: no agent has generated memory yet. Start a conversation and come back.
- **Deletion fails**: the agent is writing to memory, wait a few seconds and try again.
- **The expected namespace does not appear**: check that the agent is installed (the agent must be listed under **Agents** to appear under the *Agents* category, otherwise the namespace falls into *Other*).
- **You want to empty a whole namespace or wipe everything at once**: bulk deletion from the UI is not available yet; see [Reset Apollia (factory reset)](../troubleshooting/reinitialiser-apollia-factory-reset.md) for the full reset procedure, or use the CLI: `apollia-os memory clear --agent <NAME> --confirm`.

> **Note**: To manage the **tools** available to your agents (web search, file reading, and so on), open **Settings → Tools**. The page offers the detail of each tool, its enabling and disabling, and its optional configuration.

> **Technical reference:** [Apollia reference](/reference) - memory types, default retention durations, limits.
