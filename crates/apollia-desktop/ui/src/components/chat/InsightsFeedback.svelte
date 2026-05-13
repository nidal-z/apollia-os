<!--
  Side panel (Sheet) for reviewing insights extracted from a chat session.
  Allows the user to accept, edit, or reject each insight individually,
  or batch-process all at once.

  adds a "Rejected" tab that keeps the rejection reason
  visible for audit.
-->
<script lang="ts">
  import { t } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { uiMode } from "$lib/stores/mode";
  import {
    extractedInsights,
    rejectedInsights,
    clearInsights,
  } from "$lib/stores/chat";
  import { Sheet, SheetContent } from "$lib/components/ui/sheet";
  import { addToast } from "$lib/components/ui/toast/store";
  import InsightEntryRow from "../memory/InsightEntryRow.svelte";
  import { Card } from "$lib/components/ui/card";
  import { TabBar } from "$lib/components/ui/tabs";

  interface Props {
    open: boolean;
    onclose: () => void;
  }

  let { open, onclose }: Props = $props();

  let closing = $state(false);
  let tab = $state<"pending" | "rejected">("pending");

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

  function closeWithDelay(): void {
    closing = true;
    setTimeout(() => {
      closing = false;
      onclose();
    }, 2000);
  }
</script>

<Sheet {open} onclose={onclose}>
  <SheetContent
    padding="flush"
    class="flex flex-col gap-4 p-6 pt-10"
    data-testid="insights-feedback-panel"
  >
    <!-- Header -->
    <p class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50">
      {panelTitle}
    </p>

    <!-- Tabs -->
    <TabBar
      variant="underline"
      testidPrefix="insights"
      activeTab={tab}
      items={[
        { key: "pending", label: $t("memory.insights.tab_pending"), count: $extractedInsights.length },
        { key: "rejected", label: $t("memory.insights.tab_rejected"), count: $rejectedInsights.length },
      ]}
      ontabchange={(key) => (tab = key as "pending" | "rejected")}
    />

    {#if tab === "pending"}
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
        <!-- Batch actions — only Accept-all; reject requires a reason per entry. -->
        {#if $extractedInsights.length >= 2}
          <div class="flex items-center gap-2">
            <button
              class="rounded-md px-3 py-1.5 text-xs font-medium text-emerald-600 border border-emerald-500/30 hover:bg-emerald-500/10 transition-colors"
              onclick={acceptAll}
              data-testid="accept-all"
            >
              {acceptAllLabel}
            </button>
            <span
              class="text-[10px] text-muted-foreground/70"
              title={$t("memory.insights.reject_all_disabled_hint")}
              data-testid="reject-all-disabled"
            >
              {rejectAllLabel} — {$t("memory.insights.reject_all_disabled_hint")}
            </span>
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
    {:else if $rejectedInsights.length === 0}
      <div class="flex flex-1 items-center justify-center">
        <p class="text-sm text-muted-foreground">
          {$t("memory.insights.rejected_empty")}
        </p>
      </div>
    {:else}
      <div class="space-y-2" data-testid="rejected-list">
        {#each $rejectedInsights as entry (entry.id)}
          <Card class="rounded-lg p-3 space-y-1.5">
            <p class="text-sm text-muted-foreground line-through decoration-red-500/60">
              {entry.text}
            </p>
            <p class="text-[11px]">
              <span class="font-medium text-muted-foreground/70">
                {$t("memory.insights.reject_reason_label")}:
              </span>
              <span class="text-foreground">{entry.rejected_reason}</span>
            </p>
            {#if entry.source_quote}
              <blockquote class="border-l-2 border-muted pl-2 text-[11px] italic text-muted-foreground/80">
                {entry.source_quote}
              </blockquote>
            {/if}
            <p class="text-[10px] text-muted-foreground/60">
              {new Date(entry.rejected_at).toLocaleString()}
            </p>
          </Card>
        {/each}
      </div>
    {/if}
  </SheetContent>
</Sheet>
