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

   ![Memory page: the namespace list on the left, the type filters and the search in the middle, and the entries of the user profile memory](/img/operator-help/memoire-consulter-et-nettoyer-la-memoire-1.png)

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

   ![Detail panel of a memory entry, with its value, its metadata and the Copy and Delete actions](/img/operator-help/memoire-consulter-et-nettoyer-la-memoire-2.png)

5. To **search**, type a few keywords in the **search bar** at the top of the central panel. Matching entries are shown sorted by relevance (BM25 score), and the breadcrumb reads "*N results*".

6. To **delete a specific entry**, two equivalent options:
   - Hover the row, click the **⋯** menu at the end of the row, choose **Delete**, then confirm on the **Confirm** button that appears.
   - Or open the detail panel (click the row) and use the **Delete** button at the bottom.

   The entry disappears immediately and will not come back.

## Export, import and purge a namespace

Three buttons sit at the top right of the central panel, next to the search field: **Export**, **Import** and **Purge**. They act on the **selected namespace** and are disabled until one is selected.

### Export

Click **Export**. A save dialog opens with a name already filled in (`<namespace>-memory-<date>.json`). Confirm, and Apollia writes a JSON file holding the episodic, semantic and procedural entries of the namespace. The confirmation states how many entries of each type were written, and where.

Nothing is sent anywhere: the file lands exactly where you asked, on your machine.

### Import

Click **Import**. The window asks for two things:

- **Source file** - a JSON file produced by an Apollia export, chosen through **Choose a file**.
- **Strategy** - **Merge** adds the entries that are missing and leaves the existing ones untouched; **Replace** empties the namespace first, then loads the file.

**Replace** shows a red warning in the window and asks for a second confirmation (**Empty and import**) before anything is deleted. Merge runs straight from the **Import** button. The confirmation says how many entries were loaded and under which strategy, and the list under it reloads.

### Purge by age

Click **Purge**. This is the bulk deletion, and it is irreversible, so it comes in two steps.

1. Choose the **Memory type** (All types, Episodic, Semantic, Procedural) and the age in days under **Older than (days)**. `0` deletes every entry of the chosen type.

2. Under the fields, a preview says how many of the listed entries would go. It is counted on the entries this page reads, which is why it is labelled as such: the exact figure is the one reported once the purge has run.

3. Click **Continue**, read the summary (type, namespace, age), then confirm with **Purge**. A message reports the number of entries actually removed.

## Verification

The deleted entry no longer appears in the list, even after reloading the page. A keyword search on that entry no longer returns a result. The namespace counter in the sidebar and the type counter in the segmented control are decremented.

## If it does not work

- **The page is empty**: no agent has generated memory yet. Start a conversation and come back.
- **Deletion fails**: the agent is writing to memory, wait a few seconds and try again.
- **The expected namespace does not appear**: check that the agent is installed (the agent must be listed under **Agents** to appear under the *Agents* category, otherwise the namespace falls into *Other*).
- **You want to empty a whole namespace**: use **Purge** with **All types** and `0` days. To wipe every namespace at once, see [Reset Apollia (factory reset)](../troubleshooting/reinitialiser-apollia-factory-reset.md), or use the CLI: `apollia-os memory clear --agent <NAME> --confirm`.
- **The purge preview announces fewer entries than the purge removes**: the preview is counted on the listing this page reads, which stops at the 500 most recent conversation episodes. On a large namespace the real figure is higher, and it is the one reported at the end.
- **The Export, Import and Purge buttons are greyed out**: no namespace is selected. Pick one in the left sidebar.

> **Note**: To manage the **tools** available to your agents (web search, file reading, and so on), open **Settings → Tools**. The page offers the detail of each tool, its enabling and disabling, its optional configuration and its contract, see [Inspect a tool](../controle/inspecter-un-outil.md).

> **Technical reference:** [Apollia reference](/reference) - memory types, default retention durations, limits.
