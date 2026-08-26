<script lang="ts">
  /**
   * Headless command palette dialog.
   *
   * Bind `open`, feed `groups`, and handle `onexecute` for telemetry.
   * The panel is the expressive focal point: `.glass-panel-strong`
   * + `.glow-primary` + brand rim. Fuzzy ranking lives in `./filter`.
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
  import { rankGroups } from "./filter";
  import CommandItemRow from "./CommandItem.svelte";
  import CommandGroup from "./CommandGroup.svelte";
  import CommandEmpty from "./CommandEmpty.svelte";
  import CommandFooter from "./CommandFooter.svelte";
  import Keycap from "./Keycap.svelte";
  import type { CommandPaletteGroup } from "./types";

  interface Props {
    open?: boolean;
    groups: CommandPaletteGroup[];
    placeholder?: string;
    /** Fires after an item's own `action` runs - for telemetry. */
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

  const filteredGroups = $derived(rankGroups(groups, query));
  const flatItems = $derived(filteredGroups.flatMap((g) => g.items));

  const resolvedPlaceholder = $derived(placeholder ?? $t("command.placeholder"));
  const recentLabel = $derived($t("commandPalette.groups.recent"));
  const isSearching = $derived(query.trim().length > 0);
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
    } else if (event.key === "ArrowDown" && flatItems.length > 0) {
      event.preventDefault();
      activeIndex = (activeIndex + 1) % flatItems.length;
      scrollActiveIntoView();
    } else if (event.key === "ArrowUp" && flatItems.length > 0) {
      event.preventDefault();
      activeIndex = (activeIndex - 1 + flatItems.length) % flatItems.length;
      scrollActiveIntoView();
    } else if (event.key === "Enter") {
      event.preventDefault();
      const target = flatItems[activeIndex];
      if (target) execute(target);
    } else if (event.key === "Tab") {
      // Trap focus in the input - Tab must not escape the palette.
      event.preventDefault();
    }
  }

  function scrollActiveIntoView() {
    tick().then(() => {
      if (!listboxRef || !activeId) return;
      listboxRef
        .querySelector<HTMLElement>(`#${CSS.escape(activeId)}`)
        ?.scrollIntoView({ block: "nearest" });
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
        "glass-panel-strong glow-primary flex w-[min(40rem,90vw)] flex-col overflow-hidden rounded-lg border border-border text-card-foreground",
      )}
      onclick={(e) => e.stopPropagation()}
      role="presentation"
    >
      <label class="flex items-center gap-2.5 px-3.5 py-3 text-sm">
        <span class="text-muted-foreground" aria-hidden="true">
          <SearchIcon size={15} strokeWidth={1.75} />
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
          class="min-w-0 flex-1 bg-transparent text-sm text-card-foreground caret-primary placeholder:text-muted-foreground focus:outline-none"
          data-testid="command-palette-input"
        />
        {#if isSearching}
          <span class="count" aria-hidden="true">
            {$t("command.results_count", {
              values: { count: flatItems.length },
            })}
          </span>
        {:else}
          <span aria-hidden="true"><Keycap label="esc" /></span>
        {/if}
      </label>

      <Separator variant="subtle" />

      <div
        bind:this={listboxRef}
        id="command-palette-listbox"
        role="listbox"
        aria-label={resolvedPlaceholder}
        class="max-h-[min(50vh,26.25rem)] overflow-y-auto p-1.5"
      >
        {#if filteredGroups.length === 0}
          <CommandEmpty />
        {:else}
          {#each filteredGroups as group, gi (group.label)}
            {@const baseOffset = itemIndexOffset(gi)}
            {#if gi > 0}
              <Separator variant="subtle" class="my-1" />
            {/if}
            <CommandGroup
              label={group.label}
              badge={!isSearching && group.label === recentLabel
                ? "MRU"
                : undefined}
            >
              {#each group.items as item, ii (item.id)}
                {@const absIndex = baseOffset + ii}
                <CommandItemRow
                  id={`command-item-${item.id}`}
                  {item}
                  {query}
                  active={absIndex === activeIndex}
                  onselect={() => execute(item)}
                  onhover={() => (activeIndex = absIndex)}
                />
              {/each}
            </CommandGroup>
          {/each}
        {/if}
      </div>

      <CommandFooter />
    </div>
  </div>
{/if}

<style>
  .count {
    flex: none;
    font-family: ui-monospace, "SF Mono", SFMono-Regular, Menlo, monospace;
    font-size: var(--text-micro-lg);
    color: hsl(var(--faint-foreground));
  }
</style>
