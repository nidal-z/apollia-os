<script lang="ts">
  /**
   * Headless command palette dialog (US-SP42-017).
   *
   * Bind `open`, feed `groups`, and handle `onexecute` for telemetry.
   * The caller owns registration; this component only filters + renders
   * the already-grouped `CommandItem` list.
   */
  import { tick } from "svelte";
  import { fade, scale, type TransitionConfig } from "svelte/transition";
  import { backOut } from "svelte/easing";
  import { t } from "svelte-i18n";
  import { Search as SearchIcon } from "lucide-svelte";
  import { cn } from "$lib/utils";
  import { prefersReducedMotion } from "$lib/design/motion";
  import { Separator } from "$lib/components/ui/separator";
  import type { CommandItem } from "$lib/stores/commandPalette";
  import { fuzzyMatch } from "$lib/utils/fuzzy";
  import CommandItemRow from "./CommandItem.svelte";
  import CommandGroup from "./CommandGroup.svelte";
  import type { CommandPaletteGroup } from "./types";

  interface Props {
    open?: boolean;
    groups: CommandPaletteGroup[];
    placeholder?: string;
    /** Fires after an item's own `action` runs — for telemetry. */
    onexecute?: (id: string) => void;
  }

  let {
    open = $bindable(false),
    groups,
    placeholder,
    onexecute,
  }: Props = $props();

  let query = $state("");
  let activeIndex = $state(0);
  let inputRef = $state<HTMLInputElement | null>(null);
  let listboxRef = $state<HTMLDivElement | null>(null);
  let wasOpen = false;
  let previouslyFocused: HTMLElement | null = null;

  interface RankedItem {
    item: CommandItem;
    score: number;
  }

  interface RankedGroup {
    label: string;
    items: RankedItem[];
  }

  // Filter + rank each group; drop groups with no matches.
  const filteredGroups = $derived.by<RankedGroup[]>(() => {
    const q = query.trim();
    const out: RankedGroup[] = [];
    for (const g of groups) {
      const ranked: RankedItem[] = [];
      for (const item of g.items) {
        const score = fuzzyMatch(item.label, item.keywords, q);
        if (score !== null) ranked.push({ item, score });
      }
      if (q) ranked.sort((a, b) => b.score - a.score);
      if (ranked.length > 0) out.push({ label: g.label, items: ranked });
    }
    return out;
  });

  // Flat list for keyboard navigation — same order as rendering.
  const flatItems = $derived(
    filteredGroups.flatMap((g) => g.items.map((r) => r.item)),
  );

  const resolvedPlaceholder = $derived(
    placeholder ?? $t("command.placeholder"),
  );
  const activeId = $derived(
    flatItems[activeIndex]
      ? `command-item-${flatItems[activeIndex].id}`
      : undefined,
  );

  $effect(() => {
    // Clamp selection when the filter changes.
    void filteredGroups;
    if (activeIndex >= flatItems.length) activeIndex = 0;
  });

  $effect(() => {
    if (open && !wasOpen) {
      previouslyFocused = document.activeElement as HTMLElement | null;
      query = "";
      activeIndex = 0;
      tick().then(() => inputRef?.focus());
    } else if (!open && wasOpen) {
      previouslyFocused?.focus();
      previouslyFocused = null;
    }
    wasOpen = open;
  });

  function execute(item: CommandItem) {
    open = false;
    Promise.resolve(item.action()).finally(() => onexecute?.(item.id));
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      open = false;
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      if (flatItems.length === 0) return;
      activeIndex = (activeIndex + 1) % flatItems.length;
      scrollActiveIntoView();
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      if (flatItems.length === 0) return;
      activeIndex =
        (activeIndex - 1 + flatItems.length) % flatItems.length;
      scrollActiveIntoView();
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      const target = flatItems[activeIndex];
      if (target) execute(target);
      return;
    }
    if (event.key === "Tab") {
      // Keep focus trapped in the input — Tab must not escape the palette.
      event.preventDefault();
    }
  }

  function scrollActiveIntoView() {
    tick().then(() => {
      if (!listboxRef || !activeId) return;
      const el = listboxRef.querySelector<HTMLElement>(`#${CSS.escape(activeId)}`);
      el?.scrollIntoView({ block: "nearest" });
    });
  }

  function dialogTransition(node: Element): TransitionConfig {
    if (prefersReducedMotion()) return fade(node, { duration: 150 });
    return scale(node, { start: 0.97, duration: 200, easing: backOut });
  }

  function onBackdropClick(event: MouseEvent) {
    if (event.target === event.currentTarget) open = false;
  }

  // Running offset used to compute the listbox option id per row.
  function itemIndexOffset(groupIdx: number): number {
    let offset = 0;
    for (let i = 0; i < groupIdx; i++) offset += filteredGroups[i].items.length;
    return offset;
  }
</script>

{#if open}
  <div
    class="fixed inset-0 backdrop-warm"
    style="z-index: var(--z-backdrop);"
    role="presentation"
    transition:fade={{ duration: 150 }}
  ></div>

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 flex items-start justify-center p-4 pt-[20vh]"
    style="z-index: var(--z-overlay);"
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    aria-label={resolvedPlaceholder}
    onclick={onBackdropClick}
    onkeydown={handleKeydown}
    transition:dialogTransition
    data-testid="command-palette"
  >
    <div
      class={cn(
        "flex w-[min(640px,90vw)] flex-col overflow-hidden rounded-lg border border-border bg-card text-card-foreground shadow-lg",
      )}
      onclick={(e) => e.stopPropagation()}
      role="presentation"
    >
      <label
        class="flex items-center gap-2 px-3.5 py-2.5 text-sm text-card-foreground"
      >
        <span class="text-muted-foreground" aria-hidden="true">
          <SearchIcon size={14} strokeWidth={1.75} />
        </span>
        <input
          bind:this={inputRef}
          type="text"
          role="combobox"
          aria-expanded="true"
          aria-controls="command-palette-listbox"
          aria-activedescendant={activeId}
          aria-autocomplete="list"
          placeholder={resolvedPlaceholder}
          bind:value={query}
          class="flex-1 bg-transparent text-sm text-card-foreground placeholder:text-muted-foreground focus:outline-none"
          data-testid="command-palette-input"
        />
      </label>

      <Separator variant="subtle" />

      <div
        bind:this={listboxRef}
        id="command-palette-listbox"
        role="listbox"
        aria-label={resolvedPlaceholder}
        class="max-h-[min(50vh,420px)] overflow-y-auto p-1"
      >
        {#if filteredGroups.length === 0}
          <div
            class="px-3 py-10 text-center text-sm text-muted-foreground"
            role="status"
          >
            {$t("command.empty")}
          </div>
        {:else}
          {#each filteredGroups as group, gi (group.label)}
            {@const baseOffset = itemIndexOffset(gi)}
            {#if gi > 0}
              <Separator variant="subtle" class="my-1" />
            {/if}
            <CommandGroup label={group.label}>
              {#each group.items as entry, ii (entry.item.id)}
                {@const absIndex = baseOffset + ii}
                <CommandItemRow
                  id={`command-item-${entry.item.id}`}
                  item={entry.item}
                  active={absIndex === activeIndex}
                  onselect={() => execute(entry.item)}
                  onhover={() => (activeIndex = absIndex)}
                />
              {/each}
            </CommandGroup>
          {/each}
        {/if}
      </div>
    </div>
  </div>
{/if}
