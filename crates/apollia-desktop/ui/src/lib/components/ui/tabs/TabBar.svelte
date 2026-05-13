<script lang="ts">
  import { cn } from "$lib/utils";

  interface TabItem {
    key: string;
    label: string;
    count?: number;
  }

  interface Props {
    items: TabItem[];
    activeTab: string;
    ontabchange: (key: string) => void;
    testidPrefix: string;
    /**
     * Visual style :
     * - `pill` (default) — segmented control with a pill-shaped background.
     * - `underline` — flat tabs with an active border-bottom indicator.
     */
    variant?: "pill" | "underline";
    class?: string;
  }

  let {
    items,
    activeTab,
    ontabchange,
    testidPrefix,
    variant = "pill",
    class: className = "",
  }: Props = $props();

  let tabRefs: HTMLButtonElement[] = $state([]);

  function handleKeydown(event: KeyboardEvent, index: number) {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      ontabchange(items[index].key);
      return;
    }

    let nextIndex = index;
    if (event.key === "ArrowRight") {
      nextIndex = (index + 1) % items.length;
    } else if (event.key === "ArrowLeft") {
      nextIndex = (index - 1 + items.length) % items.length;
    } else if (event.key === "Home") {
      nextIndex = 0;
    } else if (event.key === "End") {
      nextIndex = items.length - 1;
    } else {
      return;
    }
    event.preventDefault();
    tabRefs[nextIndex]?.focus();
    ontabchange(items[nextIndex].key);
  }
</script>

<div
  role="tablist"
  class={cn(
    variant === "pill"
      ? "flex gap-1 overflow-x-auto rounded-md border border-border/50 bg-muted/50 p-1 scrollbar-none"
      : "flex gap-1 overflow-x-auto border-b border-border/60 scrollbar-none",
    className,
  )}
  data-testid="{testidPrefix}-tabbar"
>
  {#each items as item, i}
    {@const active = item.key === activeTab}
    <button
      role="tab"
      aria-selected={active}
      tabindex={active ? 0 : -1}
      class={cn(
        "shrink-0 whitespace-nowrap transition-all",
        variant === "pill"
          ? cn(
              "rounded-md px-3 py-1.5 text-sm font-medium",
              active
                ? "bg-background text-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground",
            )
          : cn(
              "px-3 py-2 text-[12.5px] -mb-px border-b-2 bg-transparent",
              active
                ? "text-foreground font-semibold border-primary"
                : "text-muted-foreground font-medium border-transparent hover:text-foreground",
            ),
      )}
      onclick={() => ontabchange(item.key)}
      onkeydown={(e) => handleKeydown(e, i)}
      bind:this={tabRefs[i]}
      data-testid="{testidPrefix}-tab-{item.key}"
    >
      {item.label}{#if item.count !== undefined}&nbsp;({item.count}){/if}
    </button>
  {/each}
</div>
