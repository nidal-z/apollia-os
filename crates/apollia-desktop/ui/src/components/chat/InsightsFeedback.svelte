<!--
  Side panel (Sheet) for reviewing insights extracted from a chat session.
  Allows the user to accept, edit, or reject each insight individually,
  or batch-process all at once.
-->
<script lang="ts">
  import { t } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { uiMode } from "$lib/stores/mode";
  import { extractedInsights, clearInsights } from "$lib/stores/chat";
  import { Sheet } from "$lib/components/ui/sheet";
  import { addToast } from "$lib/components/ui/toast/store";
  import InsightEntryRow from "../memory/InsightEntryRow.svelte";

  interface Props {
    open: boolean;
    onclose: () => void;
  }

  let { open, onclose }: Props = $props();

  let closing = $state(false);

  const isOperator = $derived($uiMode === "operator");

  const panelTitle = $derived(
    isOperator
      ? $t("memory.insights.panel_title_operator")
      : $t("memory.insights.panel_title_builder"),
  );

  const acceptAllLabel = $derived(
    isOperator
      ? $t("memory.insights.accept_all_operator")
      : $t("memory.insights.accept_all_builder"),
  );

  const rejectAllLabel = $derived(
    isOperator
      ? $t("memory.insights.reject_all_operator")
      : $t("memory.insights.reject_all_builder"),
  );

  function handleRemove(id: string): void {
    extractedInsights.update((list) => list.filter((i) => i.id !== id));
  }

  function handleUpdate(id: string, text: string, category: string): void {
    extractedInsights.update((list) =>
      list.map((i) => (i.id === id ? { ...i, text, category: category as "preference" | "habit" | "context" } : i)),
    );
  }

  async function acceptAll(): Promise<void> {
    const ids = $extractedInsights.map((i) => i.id);
    try {
      for (const id of ids) {
        await invoke("accept_extracted_insight", { id });
      }
      addToast($t("memory.insights.batch_accepted"), "success");
      clearInsights();
      closeWithDelay();
    } catch (e) {
      addToast(`${$t("memory.insights.accept_failed")}: ${e}`, "error");
    }
  }

  async function rejectAll(): Promise<void> {
    const ids = $extractedInsights.map((i) => i.id);
    try {
      for (const id of ids) {
        await invoke("reject_extracted_insight", { id });
      }
      addToast($t("memory.insights.batch_rejected"), "success");
      clearInsights();
      closeWithDelay();
    } catch (e) {
      addToast(`${$t("memory.insights.reject_failed")}: ${e}`, "error");
    }
  }

  function closeWithDelay(): void {
    closing = true;
    setTimeout(() => {
      closing = false;
      onclose();
    }, 2000);
  }
</script>

<Sheet {open} onclose={onclose}>
  <div
    class="flex h-full flex-col gap-4 overflow-y-auto p-6 pt-10"
    data-testid="insights-feedback-panel"
  >
    <!-- Header -->
    <p class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50">
      {panelTitle}
    </p>

    {#if closing}
      <div class="flex flex-1 items-center justify-center">
        <p class="text-sm text-muted-foreground animate-fade-in">
          {$t("memory.insights.batch_accepted")}
        </p>
      </div>
    {:else if $extractedInsights.length === 0}
      <div class="flex flex-1 items-center justify-center">
        <p class="text-sm text-muted-foreground">
          {$t("memory.insights.batch_accepted")}
        </p>
      </div>
    {:else}
      <!-- Batch actions -->
      {#if $extractedInsights.length >= 2}
        <div class="flex items-center gap-2">
          <button
            class="rounded-md px-3 py-1.5 text-xs font-medium text-emerald-600 border border-emerald-500/30 hover:bg-emerald-500/10 transition-colors"
            onclick={acceptAll}
            data-testid="accept-all"
          >
            {acceptAllLabel}
          </button>
          <button
            class="rounded-md px-3 py-1.5 text-xs font-medium text-red-500 border border-red-500/30 hover:bg-red-500/10 transition-colors"
            onclick={rejectAll}
            data-testid="reject-all"
          >
            {rejectAllLabel}
          </button>
        </div>
      {/if}

      <!-- Insight list -->
      <div class="space-y-2">
        {#each $extractedInsights as insight (insight.id)}
          <InsightEntryRow
            {insight}
            onremove={handleRemove}
            onupdate={handleUpdate}
          />
        {/each}
      </div>
    {/if}
  </div>
</Sheet>
