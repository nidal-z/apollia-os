<!--
  Single insight row with accept/edit/reject actions.
  Shared by InsightsFeedback (Sheet) and RecentExtractions (Memory page).

  exposes `source_quote` (collapsible) and
  `extraction_reasoning` (info tooltip), and forces a rejection reason
  through an inline textarea before confirming the reject.
-->
<script lang="ts">
  import { t } from "svelte-i18n";
  import {
    acceptExtractedInsight,
    rejectExtractedInsight,
    updateExtractedInsight,
  } from "$lib/ipc/memory";
  import type { InsightEntry } from "$lib/types";
  import { addToast } from "$lib/components/ui/toast/store";
  import { recordRejectedInsight } from "$lib/stores/chat";
  import { Check, Pencil, X, Info, ChevronDown, ChevronUp } from "lucide-svelte";
  import { Card } from "$lib/components/ui/card";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";
  import { Textarea } from "$lib/components/ui/textarea";
  import { FormField } from "$lib/components/ui/form-field";

  interface Props {
    insight: InsightEntry;
    onremove: (id: string) => void;
    onupdate: (id: string, text: string, category: string) => void;
  }

  let { insight, onremove, onupdate }: Props = $props();

  function categoryClass(cat: string): string {
    return (
      ({
        preference: "bg-primary",
        habit: "bg-secondary",
        context: "bg-warning",
      } as Record<string, string>)[cat] ?? "bg-muted"
    );
  }

  const CATEGORIES = ["preference", "habit", "context"] as const;

  let editing = $state(false);
  let editText = $state("");
  let editCategory = $state("");

  let quoteOpen = $state(false);
  let rejecting = $state(false);
  let rejectReason = $state("");

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
      await acceptExtractedInsight(insight.id);
      onremove(insight.id);
    } catch (e) {
      addToast(`${$t("memory.insights.accept_failed")}: ${e}`, "error");
    }
  }

  function startReject(): void {
    rejecting = true;
    rejectReason = "";
  }

  function cancelReject(): void {
    rejecting = false;
    rejectReason = "";
  }

  async function confirmReject(): Promise<void> {
    const reason = rejectReason.trim();
    if (reason === "") return;
    try {
      await rejectExtractedInsight(insight.id, reason);
      recordRejectedInsight(insight, reason);
      onremove(insight.id);
    } catch (e) {
      addToast(`${$t("memory.insights.reject_failed")}: ${e}`, "error");
    }
  }

  async function saveEdit(): Promise<void> {
    if (editText.trim() === "") return;
    try {
      await updateExtractedInsight(insight.id, editText.trim(), editCategory);
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

<Card class="rounded-lg p-3 transition-all duration-150" data-testid="insight-entry">
  {#if editing}
    <!-- Edit mode -->
    <div class="space-y-2">
      <Input
        type="text"
        class="w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm ring-offset-background transition-shadow focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
        bind:value={editText}
       />
      <Select
        class="rounded-md border border-border bg-background px-3 py-1.5 text-sm"
        bind:value={editCategory}
      >
        {#each CATEGORIES as cat}
          <option value={cat}>{$t(`memory.insights.category_${cat}`)}</option>
        {/each}
      </Select>
      <div class="flex gap-2 justify-end">
        <button
          class="rounded-md px-2.5 py-1 text-xs font-medium text-muted-foreground hover:bg-muted/50 transition-colors"
          onclick={cancelEdit}
        >
          {$t("memory.insights.cancel")}
        </button>
        <button
          class="rounded-md px-2.5 py-1 text-xs font-medium text-primary-foreground bg-primary hover:bg-primary/90 transition-colors disabled:opacity-40"
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
          {#if insight.extraction_reasoning}
            <span
              class="group/reason relative inline-flex"
              data-testid="insight-reasoning"
            >
              <Info size={12} class="text-muted-foreground/70" aria-label={$t("memory.insights.reasoning_label")} />
              <span
                class="absolute top-full left-0 mt-1 hidden group-hover/reason:block max-w-xs text-[10px] bg-background border border-border rounded px-2 py-1 shadow-sm z-10 whitespace-normal"
              >
                <span class="block font-medium text-muted-foreground mb-0.5">
                  {$t("memory.insights.reasoning_label")}
                </span>
                {insight.extraction_reasoning}
              </span>
            </span>
          {/if}
        </div>
        <div class="flex items-center gap-2">
          <span
            class="inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-medium text-white {categoryClass(insight.category)}"
          >
            {$t(`memory.insights.category_${insight.category}`)}
          </span>
          <!-- Confidence bar -->
          <div class="group/conf relative flex items-center gap-1.5">
            <div class="h-0.5 w-16 rounded-full bg-muted/50 overflow-hidden">
              <div
                class="h-full rounded-full"
                style="width: {insight.confidence * 100}%; background: linear-gradient(to right, hsl(var(--primary)), hsl(var(--secondary)))"
              ></div>
            </div>
            <span
              class="absolute -top-7 left-1/2 -translate-x-1/2 hidden group-hover/conf:block text-[10px] bg-background border border-border rounded px-1.5 py-0.5 whitespace-nowrap shadow-sm z-10"
            >
              {confidenceLabel}
            </span>
          </div>
          {#if insight.source_quote}
            <button
              type="button"
              class="inline-flex items-center gap-0.5 text-[10px] font-medium text-muted-foreground/70 hover:text-foreground transition-colors"
              onclick={() => (quoteOpen = !quoteOpen)}
              aria-expanded={quoteOpen}
              data-testid="insight-quote-toggle"
            >
              {$t("memory.insights.source_quote_label")}
              {#if quoteOpen}<ChevronUp size={10} />{:else}<ChevronDown size={10} />{/if}
            </button>
          {/if}
        </div>

        {#if insight.source_quote && quoteOpen}
          <blockquote
            class="mt-2 border-l-2 border-muted pl-2 text-[11px] italic text-muted-foreground"
            data-testid="insight-quote"
          >
            {insight.source_quote}
          </blockquote>
        {/if}

        {#if rejecting}
          <FormField
            id="reject-reason-{insight.id}"
            label={$t("memory.insights.reject_reason_label")}
            labelClass="text-[10px] font-normal uppercase tracking-wider text-muted-foreground/60"
            class="mt-2 space-y-2"
            data-testid="insight-reject-form"
          >
            <Textarea
              id="reject-reason-{insight.id}"
              class="w-full rounded-md border border-border bg-background px-2 py-1 text-xs resize-y min-h-[48px]"
              bind:value={rejectReason}
              placeholder={$t("memory.insights.reject_reason_placeholder")}
              data-testid="insight-reject-reason"
            ></Textarea>
            <div class="flex gap-2 justify-end">
              <button
                class="rounded-md px-2.5 py-1 text-xs font-medium text-muted-foreground hover:bg-muted/50 transition-colors"
                onclick={cancelReject}
              >
                {$t("memory.insights.cancel")}
              </button>
              <button
                class="rounded-md px-2.5 py-1 text-xs font-medium text-white bg-red-500 hover:bg-red-500/90 transition-colors disabled:opacity-40"
                onclick={confirmReject}
                disabled={rejectReason.trim() === ""}
                data-testid="insight-reject-confirm"
              >
                {$t("memory.insights.reject")}
              </button>
            </div>
          </FormField>
        {/if}
      </div>

      <!-- Action buttons -->
      {#if !rejecting}
        <div class="flex items-center gap-1 shrink-0">
          <button
            class="rounded-md p-1.5 text-success hover:bg-success/10 transition-colors"
            onclick={acceptInsight}
            aria-label={$t("memory.insights.accept")}
            title={$t("memory.insights.accept")}
            data-testid="insight-accept"
          >
            <Check size={14} />
          </button>
          <button
            class="rounded-md p-1.5 text-primary hover:bg-primary/10 transition-colors"
            onclick={startEdit}
            aria-label={$t("memory.insights.edit")}
            title={$t("memory.insights.edit")}
            data-testid="insight-edit"
          >
            <Pencil size={14} />
          </button>
          <button
            class="rounded-md p-1.5 text-red-500 hover:bg-red-500/10 transition-colors"
            onclick={startReject}
            aria-label={$t("memory.insights.reject")}
            title={$t("memory.insights.reject")}
            data-testid="insight-reject"
          >
            <X size={14} />
          </button>
        </div>
      {/if}
    </div>
  {/if}
</Card>
