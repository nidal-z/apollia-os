<!--
  Single insight row with accept/edit/reject actions.
  Shared by InsightsFeedback (Sheet) and RecentExtractions (Memory page).
-->
<script lang="ts">
  import { t } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import type { InsightEntry } from "$lib/types";
  import { addToast } from "$lib/components/ui/toast/store";
  import { Check, Pencil, X } from "lucide-svelte";

  interface Props {
    insight: InsightEntry;
    onremove: (id: string) => void;
    onupdate: (id: string, text: string, category: string) => void;
  }

  let { insight, onremove, onupdate }: Props = $props();

  const CATEGORY_COLORS: Record<string, string> = {
    preference: "#3435f5",
    habit: "#7c5fd6",
    context: "#f59e0b",
  };

  const CATEGORIES = ["preference", "habit", "context"] as const;

  let editing = $state(false);
  let editText = $state("");
  let editCategory = $state("");

  function startEdit(): void {
    editText = insight.text;
    editCategory = insight.category;
    editing = true;
  }

  function cancelEdit(): void {
    editing = false;
  }

  async function acceptInsight(): Promise<void> {
    try {
      await invoke("accept_extracted_insight", { id: insight.id });
      onremove(insight.id);
    } catch (e) {
      addToast(`${$t("memory.insights.accept_failed")}: ${e}`, "error");
    }
  }

  async function rejectInsight(): Promise<void> {
    try {
      await invoke("reject_extracted_insight", { id: insight.id });
      onremove(insight.id);
    } catch (e) {
      addToast(`${$t("memory.insights.reject_failed")}: ${e}`, "error");
    }
  }

  async function saveEdit(): Promise<void> {
    if (editText.trim() === "") return;
    try {
      await invoke("update_extracted_insight", {
        id: insight.id,
        text: editText.trim(),
        category: editCategory,
      });
      onupdate(insight.id, editText.trim(), editCategory);
      editing = false;
    } catch (e) {
      addToast(`${$t("memory.insights.update_failed")}: ${e}`, "error");
    }
  }

  let confidenceLabel = $derived(
    insight.confidence >= 1.0
      ? $t("memory.insights.confidence_explicit", { values: { score: insight.confidence.toFixed(1) } })
      : $t("memory.insights.confidence_inferred", { values: { score: insight.confidence.toFixed(1) } }),
  );
</script>

<div
  class="rounded-lg glass-card glass-border p-3 transition-all duration-150"
  data-testid="insight-entry"
>
  {#if editing}
    <!-- Edit mode -->
    <div class="space-y-2">
      <input
        type="text"
        class="w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm ring-offset-background transition-shadow focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
        bind:value={editText}
      />
      <select
        class="rounded-md border border-border bg-background px-3 py-1.5 text-sm"
        bind:value={editCategory}
      >
        {#each CATEGORIES as cat}
          <option value={cat}>{$t(`memory.insights.category_${cat}`)}</option>
        {/each}
      </select>
      <div class="flex gap-2 justify-end">
        <button
          class="rounded-md px-2.5 py-1 text-xs font-medium text-muted-foreground hover:bg-muted/50 transition-colors"
          onclick={cancelEdit}
        >
          {$t("memory.insights.cancel")}
        </button>
        <button
          class="rounded-md px-2.5 py-1 text-xs font-medium text-white bg-[#3435f5] hover:bg-[#3435f5]/90 transition-colors disabled:opacity-40"
          onclick={saveEdit}
          disabled={editText.trim() === ""}
          data-testid="insight-save"
        >
          {$t("memory.insights.save")}
        </button>
      </div>
    </div>
  {:else}
    <!-- Read mode -->
    <div class="flex items-start gap-3">
      <div class="flex-1 min-w-0">
        <div class="flex items-center gap-2 flex-wrap mb-1.5">
          <p class="text-sm text-foreground">{insight.text}</p>
        </div>
        <div class="flex items-center gap-2">
          <span
            class="inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-medium text-white"
            style="background-color: {CATEGORY_COLORS[insight.category] ?? '#6b7280'}"
          >
            {$t(`memory.insights.category_${insight.category}`)}
          </span>
          <!-- Confidence bar -->
          <div class="group/conf relative flex items-center gap-1.5">
            <div class="h-0.5 w-16 rounded-full bg-muted/50 overflow-hidden">
              <div
                class="h-full rounded-full"
                style="width: {insight.confidence * 100}%; background: linear-gradient(to right, #3435f5, #7c5fd6)"
              ></div>
            </div>
            <span
              class="absolute -top-7 left-1/2 -translate-x-1/2 hidden group-hover/conf:block text-[10px] bg-background border border-border rounded px-1.5 py-0.5 whitespace-nowrap shadow-sm z-10"
            >
              {confidenceLabel}
            </span>
          </div>
        </div>
      </div>

      <!-- Action buttons -->
      <div class="flex items-center gap-1 shrink-0">
        <button
          class="rounded-md p-1.5 text-emerald-500 hover:bg-emerald-500/10 transition-colors"
          onclick={acceptInsight}
          aria-label={$t("memory.insights.accept")}
          title={$t("memory.insights.accept")}
          data-testid="insight-accept"
        >
          <Check size={14} />
        </button>
        <button
          class="rounded-md p-1.5 text-[#3435f5] hover:bg-[#3435f5]/10 transition-colors"
          onclick={startEdit}
          aria-label={$t("memory.insights.edit")}
          title={$t("memory.insights.edit")}
          data-testid="insight-edit"
        >
          <Pencil size={14} />
        </button>
        <button
          class="rounded-md p-1.5 text-red-500 hover:bg-red-500/10 transition-colors"
          onclick={rejectInsight}
          aria-label={$t("memory.insights.reject")}
          title={$t("memory.insights.reject")}
          data-testid="insight-reject"
        >
          <X size={14} />
        </button>
      </div>
    </div>
  {/if}
</div>
