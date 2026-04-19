<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Sheet from "$lib/components/ui/sheet/Sheet.svelte";
  import type { InjectedEntry } from "$lib/types";
  import { Brain, ExternalLink } from "lucide-svelte";

  interface Props {
    /** Whether the sheet is open. */
    open: boolean;
    /** Turn id used to resolve injected entries. */
    turnId: string | null;
    /** Handler for closing the sheet. */
    onclose: () => void;
    /** Called when the user clicks an entry — typically opens InsightsPanel. */
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
      entries = await invoke<InjectedEntry[]>("get_injected_memory_entries", {
        turnId: id,
      });
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
  <header class="flex items-center gap-2 border-b border-border px-4 py-3">
    <Brain size={14} class="text-primary" />
    <h2 class="text-sm font-medium">Mémoire injectée</h2>
    {#if entries.length > 0}
      <span
        class="ml-auto rounded-full bg-primary/10 px-2 py-0.5 text-[10px] text-primary tabular-nums"
        data-testid="injected-memory-count"
      >
        {entries.length}
      </span>
    {/if}
  </header>

  <div class="flex-1 overflow-y-auto px-4 py-3" data-testid="injected-memory-sheet">
    {#if !turnId}
      <p class="text-[11px] italic text-muted-foreground/60">
        Aucun tour actif.
      </p>
    {:else if loading}
      <p class="text-[11px] text-muted-foreground/60">Chargement…</p>
    {:else if error}
      <p class="text-[11px] text-destructive" data-testid="injected-memory-error">
        {error}
      </p>
    {:else if entries.length === 0}
      <p class="text-[11px] italic text-muted-foreground/60">
        L'agent n'a injecté aucune mémoire pour ce tour.
      </p>
      <p class="mt-2 text-[10px] text-muted-foreground/50">
        Principe&nbsp;6 — la mémoire n'est jamais injectée automatiquement.
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
                <p class="flex-1 text-[11px] leading-snug text-foreground/90">
                  {entry.content_preview}
                </p>
                <ExternalLink
                  size={10}
                  class="mt-0.5 text-muted-foreground/40 transition-colors group-hover:text-primary"
                />
              </div>
              <div class="mt-2 flex items-center gap-2 text-[10px]">
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
                class="mt-1 text-[10px] italic text-muted-foreground/60"
                data-testid="injected-memory-reason"
              >
                {entry.injection_reason}
              </p>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</Sheet>
