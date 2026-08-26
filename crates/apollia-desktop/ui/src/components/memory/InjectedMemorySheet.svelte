<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { getInjectedMemoryEntries } from "$lib/ipc/memory";
  import { Sheet, SheetHeader, SheetContent } from "$lib/components/ui/sheet";
  import type { InjectedEntry } from "$lib/types";
  import { Brain, ExternalLink } from "lucide-svelte";

  interface Props {
    /** Whether the sheet is open. */
    open: boolean;
    /** Turn id used to resolve injected entries. */
    turnId: string | null;
    /** Handler for closing the sheet. */
    onclose: () => void;
    /** Called when the user clicks an entry - typically opens InsightsPanel. */
    onentryselect?: (entry: InjectedEntry) => void;
  }

  let { open, turnId, onclose, onentryselect }: Props = $props();

  let entries = $state<InjectedEntry[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  async function loadEntries(id: string | null) {
    if (!id) {
      entries = [];
      return;
    }
    loading = true;
    error = null;
    try {
      entries = await getInjectedMemoryEntries(id);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      entries = [];
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (open) {
      void loadEntries(turnId);
    }
  });

  onMount(() => {
    if (open) {
      void loadEntries(turnId);
    }
  });

  function handleEntryClick(entry: InjectedEntry) {
    onentryselect?.(entry);
  }

  function scorePct(score: number): number {
    return Math.round(Math.max(0, Math.min(1, score)) * 100);
  }
</script>

<Sheet {open} {onclose} width="md">
  <SheetHeader title={$t("memory.injected_title")} class="px-4 py-3 items-center">
    {#snippet leading()}
      <Brain size={14} class="text-primary" />
    {/snippet}
    {#snippet actions()}
      {#if entries.length > 0}
        <span
          class="rounded-full bg-primary/10 px-2 py-0.5 text-micro text-primary tabular-nums"
          data-testid="injected-memory-count"
        >
          {entries.length}
        </span>
      {/if}
    {/snippet}
  </SheetHeader>

  <SheetContent padding="flush" class="px-4 py-3" data-testid="injected-memory-sheet">
    {#if !turnId}
      <p class="text-caption italic text-muted-foreground/60">
        {$t("memory.injected_no_turn")}
      </p>
    {:else if loading}
      <p class="text-caption text-muted-foreground/60">{$t("common.loading")}</p>
    {:else if error}
      <p class="text-caption text-destructive" data-testid="injected-memory-error">
        {error}
      </p>
    {:else if entries.length === 0}
      <p class="text-caption italic text-muted-foreground/60">
        {$t("memory.injected_empty")}
      </p>
      <p class="mt-2 text-micro text-muted-foreground/50">
        {$t("memory.injected_principle")}
      </p>
    {:else}
      <ul class="flex flex-col gap-2">
        {#each entries as entry (entry.id)}
          <li>
            <button
              type="button"
              class="group w-full rounded-lg border border-border/60 bg-card/40 p-3 text-left transition-colors hover:border-primary/50 hover:bg-card"
              data-testid="injected-memory-entry"
              onclick={() => handleEntryClick(entry)}
            >
              <div class="flex items-start justify-between gap-2">
                <p class="flex-1 text-caption leading-snug text-foreground/90">
                  {entry.content_preview}
                </p>
                <ExternalLink
                  size={10}
                  class="mt-0.5 text-muted-foreground/40 transition-colors group-hover:text-primary"
                />
              </div>
              <div class="mt-2 flex items-center gap-2 text-micro">
                <span
                  class="rounded bg-muted/60 px-1.5 py-0.5 font-mono text-muted-foreground"
                  data-testid="injected-memory-namespace"
                >
                  {entry.namespace}
                </span>
                <span
                  class="tabular-nums text-muted-foreground/70"
                  data-testid="injected-memory-score"
                >
                  {scorePct(entry.relevance_score)}%
                </span>
              </div>
              <p
                class="mt-1 text-micro italic text-muted-foreground/60"
                data-testid="injected-memory-reason"
              >
                {entry.injection_reason}
              </p>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </SheetContent>
</Sheet>
