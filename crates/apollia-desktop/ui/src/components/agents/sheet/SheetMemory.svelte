<script lang="ts">
  /**
   * SheetMemory - what the agent retained: read it, search it, delete one
   * entry, or erase it all after a confirmation that says what goes and what
   * stays. The key an entry was retained under shows in developer mode only.
   */
  import { t, locale } from "svelte-i18n";
  import { Search, Trash2 } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { DeveloperOnly } from "$lib/components/shared";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { formatRelativeTime } from "$lib/utils";
  import { reportError } from "$lib/errors/reportError";
  import { clearMemory, listMemoryEntries } from "$lib/ipc/agents";
  import { deleteMemoryEntry, searchMemory } from "$lib/ipc/memory";
  import { createIdentityGuard } from "$lib/utils/identityGuard";
  import SheetSection from "./SheetSection.svelte";
  import {
    memoryRowsFromEntries,
    memoryRowsFromSearch,
    withoutRow,
    type MemoryRow,
  } from "./agentSheet";

  interface Props {
    /** Memory namespace of the agent, `null` when it keeps none of its own. */
    namespace: string | null;
  }

  let { namespace }: Props = $props();

  const SHOWN = 12;

  let rows = $state<MemoryRow[]>([]);
  let total = $state(0);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let query = $state("");
  let showAll = $state(false);
  let confirmingClear = $state(false);
  let clearing = $state(false);

  const guard = createIdentityGuard(() => `${namespace ?? ""}\u0000${query.trim()}`);

  $effect(() => {
    const ns = namespace;
    const q = query.trim();
    if (!ns) {
      rows = [];
      total = 0;
      return;
    }
    void load(ns, q);
  });

  async function load(ns: string, q: string): Promise<void> {
    const ticket = guard.begin();
    loading = true;
    error = null;
    try {
      const next =
        q.length > 0
          ? memoryRowsFromSearch(await searchMemory(ns, q))
          : memoryRowsFromEntries(await listMemoryEntries(ns));
      if (!ticket.current) return;
      rows = next;
      if (q.length === 0) total = next.length;
    } catch (err) {
      if (!ticket.current) return;
      error = reportError(err, { surface: "inline" }).friendly_message;
      rows = [];
    } finally {
      if (ticket.current) loading = false;
    }
  }

  async function remove(row: MemoryRow): Promise<void> {
    if (!namespace) return;
    try {
      await deleteMemoryEntry(namespace, row.id);
      rows = withoutRow(rows, row.id);
      total = Math.max(0, total - 1);
    } catch (err) {
      reportError(err, { surface: "toast", testid: "agent-sheet-memory-delete-error" });
    }
  }

  async function clearAll(): Promise<void> {
    if (!namespace) return;
    clearing = true;
    try {
      await clearMemory(namespace);
      rows = [];
      total = 0;
      query = "";
    } catch (err) {
      reportError(err, { surface: "toast", testid: "agent-sheet-memory-clear-error" });
    } finally {
      clearing = false;
      confirmingClear = false;
    }
  }

  const visible = $derived(showAll ? rows : rows.slice(0, SHOWN));
</script>

<SheetSection
  title={$t("agent_sheet.memory_title")}
  legend={$t("agent_sheet.memory_legend")}
  testid="agent-sheet-memory"
>
  {#snippet action()}
    {#if namespace && total > 0}
      <Button
        variant="ghost"
        size="sm"
        onclick={() => (confirmingClear = true)}
        data-testid="agent-sheet-memory-clear"
      >
        {#snippet icon()}<Trash2 size={12} />{/snippet}
        {$t("agent_sheet.memory_clear")}
      </Button>
    {/if}
  {/snippet}

  {#if !namespace}
    <p class="m-0 text-body-xs italic text-muted-foreground" data-testid="agent-sheet-memory-none">
      {$t("agent_sheet.memory_none")}
    </p>
  {:else}
    <div class="mb-3 flex items-center gap-2">
      <Search size={14} class="shrink-0 text-muted-foreground" />
      <Input
        bind:value={query}
        placeholder={$t("agent_sheet.memory_search_placeholder")}
        aria-label={$t("agent_sheet.memory_search_placeholder")}
        data-testid="agent-sheet-memory-search"
      />
    </div>
    {#if loading}
      <p class="m-0 text-body-xs text-muted-foreground">{$t("common.loading")}</p>
    {:else if error}
      <p class="m-0 text-body-xs text-destructive" data-testid="agent-memory-error">{error}</p>
    {:else if rows.length === 0}
      <p class="m-0 text-body-xs italic text-muted-foreground" data-testid="agent-sheet-memory-empty">
        {query.trim().length > 0 ? $t("agent_sheet.memory_no_match") : $t("agent_sheet.memory_empty")}
      </p>
    {:else}
      <p class="m-0 mb-2 text-caption text-muted-foreground" data-testid="agent-sheet-memory-count">
        {query.trim().length > 0
          ? $t("agent_sheet.memory_matches", { values: { count: rows.length, total } })
          : $t("agent_sheet.memory_count", { values: { count: total } })}
      </p>
      <ul class="m-0 list-none space-y-1 p-0">
        {#each visible as row (row.id)}
          <li class="flex items-start gap-2 border-b border-border/40 py-1.5 last:border-b-0" data-testid="agent-memory-row">
            <div class="min-w-0 flex-1">
              <p class="m-0 text-body-xs text-foreground/85">{row.text}</p>
              <div class="mt-0.5 flex flex-wrap items-center gap-2 text-caption text-muted-foreground">
                <span>{formatRelativeTime(row.at, $locale ?? "fr")}</span>
                {#if row.key}
                  <DeveloperOnly>
                    <code class="font-mono" data-testid="agent-sheet-memory-key">{row.key}</code>
                  </DeveloperOnly>
                {/if}
              </div>
            </div>
            <Button
              variant="ghost"
              size="sm"
              onclick={() => void remove(row)}
              data-testid="agent-sheet-memory-delete"
            >
              {$t("agent_sheet.memory_delete")}
            </Button>
          </li>
        {/each}
      </ul>
      {#if rows.length > SHOWN}
        <Button
          variant="ghost"
          size="sm"
          class="mt-2"
          onclick={() => (showAll = !showAll)}
          data-testid="agent-sheet-memory-more"
        >
          {showAll
            ? $t("agent_sheet.memory_show_less")
            : $t("agent_sheet.memory_show_all", { values: { count: rows.length } })}
        </Button>
      {/if}
    {/if}
  {/if}
</SheetSection>

<ConfirmDialog
  open={confirmingClear}
  onclose={() => (confirmingClear = false)}
  onconfirm={() => void clearAll()}
  title={$t("agent_sheet.memory_clear_title")}
  message={$t("agent_sheet.memory_clear_message", { values: { count: total } })}
  confirmLabel={$t("agent_sheet.memory_clear_confirm")}
  loading={clearing}
  data-testid="agent-sheet-memory-clear-dialog"
/>
