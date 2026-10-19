<script lang="ts">
  /**
   * AdjustSources - the sources of the registry, kind by kind, each checked
   * or not for the agent, with where its index stands. "Add a source" offers
   * a folder among the folder connections, a file of this machine, a mailbox
   * among the mailbox connections, the profile and the memory, and the code
   * folder in developer mode; a folder of this machine is connected on the
   * spot. A source added is checked in the draft and grants no tool: it is
   * searched, and a folder or a file the agent cannot open offers its
   * reading, connecting the folder of a file when none holds it.
   */
  import { t } from "svelte-i18n";
  import { Plus } from "lucide-svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { ActionMenu } from "$lib/components/ui/action-menu";
  import { Button } from "$lib/components/ui/button";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { addToast } from "$lib/components/ui/toast/store";
  import { addFolderConnection, listConnections } from "$lib/ipc/connections";
  import { listSourceFolderKinds, type NewSource, type SourceKind } from "$lib/ipc/sources";
  import { developerMode } from "$lib/stores/developerMode";
  import type { Connection } from "$lib/types";
  import SheetSection from "./SheetSection.svelte";
  import SourceRow from "./SourceRow.svelte";
  import AddSourceDialog from "./AddSourceDialog.svelte";
  import { grant, toggleSource, type AdjustDraft } from "./agentAdjust";
  import {
    ADD_CHOICE_KEYS,
    addChoices,
    existingSource,
    fileName,
    folderCandidates,
    folderToConnect,
    mailCandidates,
    readingToGrant,
    readingTools,
    sourceOn,
    sourceOnFile,
    sourceRows,
    type SourceCandidate,
    type SourceRow as Row,
  } from "./agentSources";
  import type { AgentSources } from "./useAgentSources.svelte";

  interface Props {
    draft: AdjustDraft;
    /** The registry and the indexes, read by the sheet. */
    sources: AgentSources;
    modified: boolean;
  }

  let { draft, sources, modified }: Props = $props();

  let folderKinds = $state<SourceKind[]>(["folder"]);
  let connections = $state<Connection[]>([]);
  let picking = $state<"folder" | "code_folder" | "mail" | null>(null);
  let loadingConnections = $state(false);
  let removing = $state<Row | null>(null);
  let removingBusy = $state(false);

  const rows = $derived(sourceRows(sources.registry, sources.indexes, draft.sources, true));
  const choices = $derived(addChoices(folderKinds));
  const candidates = $derived<SourceCandidate[]>(
    picking === "mail" ? mailCandidates(connections) : folderCandidates(connections),
  );

  $effect(() => {
    const on = $developerMode;
    let live = true;
    void listSourceFolderKinds(on)
      .then((kinds) => {
        if (live) folderKinds = kinds;
      })
      .catch(() => {
        if (live) folderKinds = ["folder"];
      });
    return () => {
      live = false;
    };
  });

  $effect(() => {
    let live = true;
    void listConnections()
      .then((listed) => {
        if (live) connections = listed;
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  });

  function check(id: string, on: boolean): void {
    draft.sources = toggleSource(draft.sources, id, on);
  }

  /** Adds `wanted` to the registry unless it already holds it, then checks it. */
  async function add(wanted: NewSource): Promise<void> {
    const source = existingSource(sources.registry, wanted) ?? (await sources.add(wanted));
    if (!source) {
      addToast(sources.error ?? $t("agent_sources.add_failed"), "error");
      return;
    }
    check(source.id, true);
  }

  async function pickConnection(kind: "folder" | "code_folder" | "mail"): Promise<void> {
    picking = kind;
    loadingConnections = true;
    try {
      connections = await listConnections();
    } catch {
      connections = [];
    } finally {
      loadingConnections = false;
    }
  }

  async function pickFile(): Promise<void> {
    const picked = await openDialog({
      multiple: false,
      directory: false,
      title: $t("agent_sources.pick_file_title"),
    });
    if (typeof picked === "string") await add(sourceOnFile(picked));
  }

  function choose(kind: SourceKind): void {
    if (kind === "profile" || kind === "memory") check(kind, true);
    else if (kind === "file") void pickFile();
    else void pickConnection(kind);
  }

  async function chosen(candidate: SourceCandidate): Promise<void> {
    const kind = picking;
    picking = null;
    if (kind) await add(sourceOn(kind, candidate));
  }

  async function remove(): Promise<void> {
    if (!removing) return;
    removingBusy = true;
    const id = removing.id;
    const gone = await sources.remove(id);
    removingBusy = false;
    removing = null;
    if (gone) check(id, false);
    else addToast(sources.error ?? $t("agent_sources.remove_failed"), "error");
  }

  /** The folder connection on `path`, the one there is or a new one. */
  async function connectFolder(path: string): Promise<Connection | null> {
    try {
      const known = await listConnections();
      const same = known.find((c) => c.kind === "folder" && c.path != null && c.path.replace(/[\\/]+$/, "") === path.replace(/[\\/]+$/, ""));
      const connection = same ?? (await addFolderConnection(path));
      connections = await listConnections();
      return connections.find((c) => c.id === connection.id) ?? connection;
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
      return null;
    }
  }

  /** A folder of this machine, connected then added as a source. */
  async function pickedFolder(path: string): Promise<void> {
    const kind = picking === "code_folder" ? "code_folder" : "folder";
    picking = null;
    const connection = await connectFolder(path);
    if (connection) await add(sourceOn(kind, { reference: connection.id, label: connection.label, detail: path }));
  }

  /** Connects the folder of a file source and grants its reading. */
  async function openFolderOf(path: string): Promise<void> {
    const connection = await connectFolder(path);
    if (connection) grantReading(readingTools(connection));
  }

  function grantReading(ids: string[]): void {
    let next = draft.access;
    for (const id of ids) next = grant(next, id);
    draft.access = next;
  }
</script>

<SheetSection
  title={$t("agent_sheet.sources_title")}
  legend={$t("agent_adjust.sources_legend")}
  testid="agent-sheet-sources"
  {modified}
>
  {#snippet action()}
    <ActionMenu>
      {#snippet triggerSlot(triggerProps)}
        <Button variant="outline" size="sm" {...triggerProps} data-testid="agent-sources-add">
          {#snippet icon()}<Plus size={12} />{/snippet}
          {$t("agent_sources.add")}
        </Button>
      {/snippet}
      {#snippet body({ close })}
        <ul class="m-0 flex list-none flex-col gap-0.5 p-0" role="menu" data-testid="agent-sources-add-menu">
          {#each choices as kind (kind)}
            <li role="none">
              <button
                type="button"
                role="menuitem"
                class="flex w-full flex-col rounded-sm px-2 py-1.5 text-left transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                onclick={() => {
                  close();
                  choose(kind);
                }}
                data-testid="agent-sources-add-choice"
                data-kind={kind}
              >
                <span class="text-body-xs text-foreground">{$t(ADD_CHOICE_KEYS[kind].title)}</span>
                <span class="text-caption text-muted-foreground">{$t(ADD_CHOICE_KEYS[kind].hint)}</span>
              </button>
            </li>
          {/each}
        </ul>
      {/snippet}
    </ActionMenu>
  {/snippet}
  <ul class="m-0 list-none divide-y divide-border/40 p-0">
    {#each rows as row (row.id)}
      {@const reading = readingToGrant(row, connections, draft.access)}
      {@const unconnected = reading.length > 0 ? null : folderToConnect(row, connections)}
      <SourceRow {row} ontoggle={(on) => check(row.id, on)} onremove={() => (removing = row)}>
        {#snippet note()}
          {#if unconnected !== null}
            <p class="m-0 mt-1 text-caption text-muted-foreground" data-testid="agent-source-reading-note">
              {$t("agent_sources.reading_note_file")}
              <button
                type="button"
                class="ml-1 text-primary underline underline-offset-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                onclick={() => void openFolderOf(unconnected)}
                data-testid="agent-source-connect-folder"
              >
                {$t("agent_sources.connect_folder", { values: { name: fileName(unconnected) } })}
              </button>
            </p>
          {:else if reading.length > 0}
            <p class="m-0 mt-1 text-caption text-muted-foreground" data-testid="agent-source-reading-note">
              {$t(row.kind === "file" ? "agent_sources.reading_note_file" : "agent_sources.reading_note")}
              <button
                type="button"
                class="ml-1 text-primary underline underline-offset-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                onclick={() => grantReading(reading)}
                data-testid="agent-source-grant-reading"
              >
                {$t(row.kind === "file" ? "agent_sources.grant_reading_file" : "agent_sources.grant_reading")}
              </button>
            </p>
          {/if}
        {/snippet}
      </SourceRow>
    {/each}
  </ul>
</SheetSection>

<AddSourceDialog
  open={picking !== null}
  kind={picking ?? "folder"}
  {candidates}
  loading={loadingConnections}
  onclose={() => (picking = null)}
  onchoose={(candidate) => void chosen(candidate)}
  onpick={(path) => void pickedFolder(path)}
/>

<ConfirmDialog
  open={removing !== null}
  onclose={() => (removing = null)}
  onconfirm={() => void remove()}
  title={$t("agent_sources.remove_title")}
  message={$t("agent_sources.remove_message", { values: { name: removing?.label ?? "" } })}
  confirmLabel={$t("agent_sources.remove_confirm")}
  loading={removingBusy}
  data-testid="agent-source-remove-dialog"
/>
