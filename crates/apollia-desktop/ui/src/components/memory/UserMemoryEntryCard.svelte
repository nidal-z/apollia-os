<script lang="ts">
  import { t } from "svelte-i18n";
  import type { UserMemoryEntryView } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Textarea } from "$lib/components/ui/textarea";
  import { formatRelativeTime } from "$lib/utils";
  import { MoreHorizontal } from "lucide-svelte";

  interface Props {
    entry: UserMemoryEntryView;
    onupdate: (key: string, value: string) => void;
    ondelete: (key: string) => void;
    onrecategorize: (key: string, category: string) => void;
    onvalidate?: (key: string) => void;
  }

  let { entry, onupdate, ondelete, onrecategorize, onvalidate }: Props = $props();

  const CATEGORY_COLORS: Record<string, string> = {
    preferences: "#3435f5",
    habits: "#7c5fd6",
    context: "#f59e0b",
  };

  const SOURCE_LABELS: Record<string, string> = {
    onboarding: "source_onboarding",
    chat_inference: "source_chat",
    user_explicit: "source_explicit",
    agent_observation: "source_agent",
  };

  const CATEGORIES = ["preferences", "habits", "context"] as const;

  let isEditing = $state(false);
  let isMenuOpen = $state(false);
  let isCategoryMenuOpen = $state(false);
  let editValue = $state("");

  function startEdit(): void {
    editValue = entry.value;
    isEditing = true;
    isMenuOpen = false;
  }

  function cancelEdit(): void {
    isEditing = false;
    editValue = "";
  }

  function saveEdit(): void {
    if (editValue.trim() === "") return;
    onupdate(entry.key, editValue.trim());
    isEditing = false;
    editValue = "";
  }

  function handleValidate(): void {
    isMenuOpen = false;
    onvalidate?.(entry.key);
  }

  function handleDelete(): void {
    isMenuOpen = false;
    ondelete(entry.key);
  }

  function openCategoryMenu(): void {
    isMenuOpen = false;
    isCategoryMenuOpen = true;
  }

  function selectCategory(cat: string): void {
    isCategoryMenuOpen = false;
    onrecategorize(entry.key, cat);
  }

  function confidenceColor(confidence: number): string {
    if (confidence >= 0.8) return "bg-emerald-500";
    if (confidence >= 0.5) return "bg-amber-500";
    return "bg-red-500";
  }

  function handleClickOutside(): void {
    isMenuOpen = false;
    isCategoryMenuOpen = false;
  }
</script>

<svelte:window onclick={handleClickOutside} />

<div
  class="group relative rounded-lg glass-card glass-border p-4 transition-all duration-150 hover:-translate-y-px hover:shadow-md"
  data-testid="entry-card"
>
  <div class="flex items-start justify-between gap-3">
    <!-- Content -->
    <div class="flex-1 min-w-0">
      <div class="flex items-center gap-2 flex-wrap mb-1">
        <span class="font-medium text-sm text-foreground">{entry.key}</span>
        <span
          class="inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-medium text-white"
          style="background-color: {CATEGORY_COLORS[entry.category] ?? '#6b7280'}"
        >
          {entry.category}
        </span>
        <Badge variant="secondary" class="text-[10px]">
          {$t(`memory.user_memory.${SOURCE_LABELS[entry.source] ?? "source_explicit"}`)}
        </Badge>
        {#if entry.confidence >= 0.95}
          <span
            class="inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-medium bg-emerald-500/15 text-emerald-600"
            data-testid="validated-badge-{entry.key}"
          >
            {$t("memory.user_memory.badge_validated")}
          </span>
        {/if}
      </div>

      {#if isEditing}
        <div class="mt-2 space-y-2">
          <Textarea
            bind:value={editValue}
            class="text-sm"
            aria-label={$t("memory.user_memory.edit_value_label")}
          />
          <div class="flex gap-2">
            <Button size="sm" onclick={saveEdit} disabled={editValue.trim() === ""}>
              {$t("memory.user_memory.edit_save")}
            </Button>
            <Button size="sm" variant="outline" onclick={cancelEdit}>
              {$t("memory.user_memory.edit_cancel")}
            </Button>
          </div>
        </div>
      {:else}
        <p class="text-sm text-muted-foreground mt-1 break-words">{entry.value}</p>
      {/if}

      <!-- Confidence bar -->
      <div class="mt-2 flex items-center gap-2">
        <div class="h-1 flex-1 max-w-[120px] rounded-full bg-muted/50 overflow-hidden">
          <div
            class="h-full rounded-full transition-all {confidenceColor(entry.confidence)}"
            style="width: {entry.confidence * 100}%"
          ></div>
        </div>
        <span class="text-[10px] text-muted-foreground/60">{Math.round(entry.confidence * 100)}%</span>
      </div>
    </div>

    <!-- Right side: date + menu -->
    <div class="flex flex-col items-end gap-1 shrink-0">
      <span class="text-[10px] text-muted-foreground/60">{formatRelativeTime(entry.updated_at)}</span>

      <div class="relative">
        <button
          class="p-1 rounded-md opacity-0 group-hover:opacity-100 transition-opacity text-muted-foreground hover:bg-muted/50"
          data-testid="entry-menu"
          onclick={(e: MouseEvent) => { e.stopPropagation(); isMenuOpen = !isMenuOpen; isCategoryMenuOpen = false; }}
          aria-label="Actions"
        >
          <MoreHorizontal size={14} />
        </button>

        {#if isMenuOpen}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="absolute right-0 top-full z-20 mt-1 w-40 rounded-md glass-card glass-border shadow-lg py-1"
            onclick={(e: MouseEvent) => e.stopPropagation()}
          >
            {#if onvalidate && entry.confidence < 0.95}
              <button
                class="w-full text-left px-3 py-1.5 text-xs text-emerald-600 hover:bg-muted/50 transition-colors"
                onclick={handleValidate}
                data-testid="validate-memory-{entry.key}"
              >
                {$t("memory.user_memory.menu_validate")}
              </button>
            {/if}
            <button
              class="w-full text-left px-3 py-1.5 text-xs hover:bg-muted/50 transition-colors"
              onclick={startEdit}
            >
              {$t("memory.user_memory.menu_edit")}
            </button>
            <button
              class="w-full text-left px-3 py-1.5 text-xs hover:bg-muted/50 transition-colors"
              onclick={openCategoryMenu}
            >
              {$t("memory.user_memory.menu_recategorize")}
            </button>
            <button
              class="w-full text-left px-3 py-1.5 text-xs text-red-500 hover:bg-muted/50 transition-colors"
              onclick={handleDelete}
            >
              {$t("memory.user_memory.menu_delete")}
            </button>
          </div>
        {/if}

        {#if isCategoryMenuOpen}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="absolute right-0 top-full z-20 mt-1 w-40 rounded-md glass-card glass-border shadow-lg py-1"
            onclick={(e: MouseEvent) => e.stopPropagation()}
          >
            {#each CATEGORIES as cat}
              <button
                class="w-full text-left px-3 py-1.5 text-xs transition-colors {cat === entry.category ? 'opacity-40 cursor-default' : 'hover:bg-muted/50'}"
                disabled={cat === entry.category}
                onclick={() => selectCategory(cat)}
              >
                <span class="inline-block w-2 h-2 rounded-full mr-2" style="background-color: {CATEGORY_COLORS[cat]}"></span>
                {cat}
              </button>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  </div>
</div>
