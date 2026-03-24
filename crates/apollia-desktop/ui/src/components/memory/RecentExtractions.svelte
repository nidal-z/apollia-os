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
    class="rounded-xl glass-card glass-border p-4 space-y-3"
    data-testid="recent-extractions"
  >
    <div class="flex items-center gap-2">
      <Sparkles size={14} class="text-[#7c5fd6]" />
      <h3 class="text-xs font-medium uppercase tracking-wider text-muted-foreground/60">
        {sectionTitle}
      </h3>
      <span class="rounded-full bg-[#7c5fd6]/15 px-1.5 text-[10px] font-medium text-[#7c5fd6]">
        {recentInsights.length}
      </span>
    </div>
    <div class="space-y-2">
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
