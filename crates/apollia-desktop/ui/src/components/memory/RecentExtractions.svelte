<!--
  Displays insights extracted in the last 24 hours at the top of the
  User Memory tab. Hidden when there are no recent extractions.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { UIMode } from "$lib/stores/mode";
  import type { InsightEntry } from "$lib/types";
  import { addToast } from "$lib/components/ui/toast/store";
  import InsightEntryRow from "./InsightEntryRow.svelte";
  import { Sparkles } from "lucide-svelte";

  interface Props {
    mode: UIMode;
  }

  let { mode }: Props = $props();

  const RECENT_HOURS = 24;

  let recentInsights = $state<InsightEntry[]>([]);
  let loaded = $state(false);

  const sectionTitle = $derived(
    mode === "operator"
      ? $t("memory.insights.recent_title_operator")
      : $t("memory.insights.recent_title_builder"),
  );

  async function loadRecent(): Promise<void> {
    try {
      recentInsights = await invoke("get_recent_extractions", { hours: RECENT_HOURS });
    } catch (e) {
      addToast(`${$t("memory.insights.load_recent_failed")}: ${e}`, "error");
    } finally {
      loaded = true;
    }
  }

  function handleRemove(id: string): void {
    recentInsights = recentInsights.filter((i) => i.id !== id);
  }

  function handleUpdate(id: string, text: string, category: string): void {
    recentInsights = recentInsights.map((i) =>
      i.id === id ? { ...i, text, category: category as "preference" | "habit" | "context" } : i,
    );
  }

  onMount(loadRecent);
</script>

{#if loaded && recentInsights.length > 0}
  <div
    class="rounded-xl border border-border/60 bg-card overflow-hidden"
    data-testid="recent-extractions"
  >
    <div
      class="px-4 py-2.5 border-b border-border/60 flex items-center gap-2"
    >
      <Sparkles size={12} class="text-secondary" />
      <h3 class="m-0 text-[10.5px] font-semibold tracking-[1px] uppercase text-muted-foreground/70 font-mono">
        {sectionTitle}
      </h3>
      <span class="text-[10.5px] text-muted-foreground/70 font-mono">{recentInsights.length}</span>
    </div>
    <div class="divide-y divide-border/60">
      {#each recentInsights as insight (insight.id)}
        <InsightEntryRow
          {insight}
          onremove={handleRemove}
          onupdate={handleUpdate}
        />
      {/each}
    </div>
  </div>
{/if}
