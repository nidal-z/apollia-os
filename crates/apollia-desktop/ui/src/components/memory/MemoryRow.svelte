<script lang="ts">
  import { t } from "svelte-i18n";
  import type { UserMemoryEntryView } from "$lib/types";
  import { Chip, StatusDot } from "$lib/components/operator";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Button } from "$lib/components/ui/button";
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

  const CATEGORIES = ["preferences", "habits", "context"] as const;

  type Tone = "primary" | "secondary" | "warning" | "info" | "success" | "neutral";

  const CATEGORY_TONE: Record<string, Tone> = {
    preferences: "primary",
    habits: "secondary",
    context: "warning",
  };

  const CATEGORY_DOT: Record<string, string> = {
    preferences: "hsl(var(--primary))",
    habits: "hsl(var(--secondary))",
    context: "hsl(var(--warning))",
  };

  const SOURCE_LABELS: Record<string, string> = {
    onboarding: "source_onboarding",
    chat_inference: "source_chat",
    user_explicit: "source_explicit",
    agent_observation: "source_agent",
  };

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

  function progressColor(confidence: number): string {
    if (confidence >= 0.8) return "hsl(var(--success))";
    if (confidence >= 0.5) return "hsl(var(--warning))";
    return "hsl(var(--destructive))";
  }

  function handleClickOutside(): void {
    isMenuOpen = false;
    isCategoryMenuOpen = false;
  }

  const sourceKey = $derived(SOURCE_LABELS[entry.source] ?? "source_explicit");
  const validated = $derived(entry.confidence >= 0.95);
  const categoryTone = $derived(CATEGORY_TONE[entry.category] ?? "neutral");
  const categoryDot = $derived(CATEGORY_DOT[entry.category] ?? "hsl(var(--muted-foreground))");
</script>

<svelte:window onclick={handleClickOutside} />

<div
  class="group px-4 py-3 flex items-center gap-2.5 border-b border-border/60 text-[12px] hover:bg-muted/40 transition-colors"
  data-testid="memory-row-{entry.key}"
>
  <!-- KEY + VALUE -->
  <div class="flex-[2] min-w-0">
    <div class="font-medium text-foreground truncate" title={entry.key}>
      {entry.key}
    </div>
    {#if isEditing}
      <div class="mt-1.5 space-y-1.5">
        <Textarea
          bind:value={editValue}
          class="text-[12px]"
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
      <div class="text-[11.5px] text-muted-foreground truncate mt-px" title={entry.value}>
        {entry.value}
      </div>
    {/if}
  </div>

  <!-- CATEGORY + SOURCE -->
  <div class="w-[180px] flex items-center gap-1.5 flex-wrap min-w-0">
    <Chip size="sm" tone={categoryTone}>
      {#snippet icon()}<StatusDot color={categoryDot} />{/snippet}
      {entry.category}
    </Chip>
    <Chip size="sm" tone="neutral" outline>
      {$t(`memory.user_memory.${sourceKey}`)}
    </Chip>
    {#if validated}
      <Chip size="sm" tone="success">
        {$t("memory.user_memory.badge_validated")}
      </Chip>
    {/if}
  </div>

  <!-- CONFIDENCE -->
  <div class="w-[120px]">
    <div class="h-1 rounded-full bg-muted overflow-hidden">
      <div
        class="h-full transition-all"
        style="width: {entry.confidence * 100}%; background: {progressColor(entry.confidence)};"
      ></div>
    </div>
    <span class="text-[10px] text-muted-foreground/70 font-mono">{Math.round(entry.confidence * 100)}%</span>
  </div>

  <!-- UPDATED -->
  <div class="w-[90px] text-[10.5px] text-muted-foreground text-right font-mono">
    {formatRelativeTime(entry.updated_at)}
  </div>

  <!-- ACTIONS -->
  <div class="w-[28px] relative shrink-0 flex justify-end">
    <button
      class="p-1 rounded-md opacity-0 group-hover:opacity-100 transition-opacity text-muted-foreground hover:bg-muted/70"
      data-testid="entry-menu"
      onclick={(e: MouseEvent) => { e.stopPropagation(); isMenuOpen = !isMenuOpen; isCategoryMenuOpen = false; }}
      aria-label={$t("a11y.actions_menu")}
    >
      <MoreHorizontal size={14} />
    </button>

    {#if isMenuOpen}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="absolute right-0 top-full z-20 mt-1 w-44 rounded-md border border-border bg-popover text-popover-foreground shadow-elev-3 py-1"
        onclick={(e: MouseEvent) => e.stopPropagation()}
      >
        {#if onvalidate && !validated}
          <button
            class="w-full text-left px-3 py-1.5 text-[11.5px] text-success-a11y hover:bg-muted transition-colors"
            onclick={handleValidate}
            data-testid="validate-memory-{entry.key}"
          >
            {$t("memory.user_memory.menu_validate")}
          </button>
        {/if}
        <button
          class="w-full text-left px-3 py-1.5 text-[11.5px] hover:bg-muted transition-colors"
          onclick={startEdit}
        >
          {$t("memory.user_memory.menu_edit")}
        </button>
        <button
          class="w-full text-left px-3 py-1.5 text-[11.5px] hover:bg-muted transition-colors"
          onclick={openCategoryMenu}
        >
          {$t("memory.user_memory.menu_recategorize")}
        </button>
        <button
          class="w-full text-left px-3 py-1.5 text-[11.5px] text-danger-a11y hover:bg-muted transition-colors"
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
        class="absolute right-0 top-full z-20 mt-1 w-44 rounded-md border border-border bg-popover text-popover-foreground shadow-elev-3 py-1"
        onclick={(e: MouseEvent) => e.stopPropagation()}
      >
        {#each CATEGORIES as cat}
          <button
            class="w-full text-left px-3 py-1.5 text-[11.5px] flex items-center gap-2 transition-colors {cat === entry.category ? 'opacity-40 cursor-default' : 'hover:bg-muted'}"
            disabled={cat === entry.category}
            onclick={() => selectCategory(cat)}
          >
            <StatusDot color={CATEGORY_DOT[cat]} />
            {cat}
          </button>
        {/each}
      </div>
    {/if}
  </div>
</div>
