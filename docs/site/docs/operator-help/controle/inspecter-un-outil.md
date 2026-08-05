# Inspect a tool

> For operators who want to know what a tool actually is before authorising it: what it takes as input, what it gives back, what it requires, and which credentials are stored against it.

## Prerequisites

- The application is open.
- You know the tool by name, or you can recognise it in the list (`bash_executor`, `file_write`, `http_fetch`, and so on).

## The difference with the Permissions page

Two pages, two questions:

- **Settings → Permissions** answers *"what is this tool allowed to do right now"*: the persisted rules, their scope, their revocation. See [Manage tool permissions](configurer-les-permissions-de-fichiers.md).
- **Settings → Tools** answers *"what is this tool"*: its enabling, its configuration, and its contract. That is this page.

## Read a tool's contract

1. In the sidebar, click **Settings**, then **Tools** in the left menu.

2. On the tool's card, click the **Contract** button (braces icon), or right-click the card and choose **View contract**. A panel opens on the right.

3. The panel shows, in order:

   - The **display name** and, under it, the technical identifier used everywhere else (the audit trail, the permission rules, a manifest).
   - Two badges: the **kind** of tool (native, served by an MCP server, custom) and the **version**, each shown only when the runtime reports it.
   - The **description** the tool declares about itself.
   - **Required permissions**, as badges. This section is only shown when the tool reports permissions: an absent section means "not reported", never "requires nothing".
   - **Input** and **Output**, as an indented list of fields: name, type, whether it is required or optional, its default value and its allowed values when the schema states them. A schema that is not a field list, and any case the reading cannot flatten, falls back to the raw document, one click away behind **Raw JSON** in both sections.
   - **Credentials**, the keys stored for this tool.

4. Close the panel with the ✕ in the header. Nothing is written from this panel, it reads.

A tool that is not registered in the runtime exposes no contract, and the panel says exactly that instead of showing an empty frame.

## See the configured credentials

At the bottom of the **Tools** page, a **Credentials** section lists every credential stored on this machine, one row per tool and key: the tool it belongs to, the key name, and when it was added. A counter next to the heading gives the total, and **Reload** re-reads the list.

**No value, and no fragment of a value, is ever displayed.** The keys stay encrypted on the machine; only their names and their dates travel to the interface. Setting, testing and deleting a key happen in the tool's own configuration drawer, on its card.

An empty section is the normal state on a fresh installation: it means no tool has been given a key yet.

## Verification

- The contract panel of `file_write` shows an input schema listing the fields the tool expects.
- The technical identifier shown under the panel title is the one you find in the **Tool** column of the [audit trail](../observabilite/consulter-l-audit-trail.md).

## If it does not work

- **The contract panel says the contract is unavailable**: the tool appears in the list but is not registered in the runtime. Nothing is broken on your side, there is simply nothing to describe.
- **The Input section shows raw JSON instead of a field list**: the schema is not a plain field list. The raw document is the whole contract, nothing was hidden.
- **A tool has a key configured but nothing shows in the Credentials section**: click **Reload**. The list is read when the page opens, not on every change made elsewhere.

> **Technical reference:** [Apollia reference](/reference)
