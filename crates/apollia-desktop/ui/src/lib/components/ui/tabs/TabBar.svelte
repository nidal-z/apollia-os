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
    class?: string;
  }

  let {
    items,
    activeTab,
    ontabchange,
    testidPrefix,
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
    "flex gap-1 rounded-md border border-border/50 bg-muted/50 p-1",
    className,
  )}
  data-testid="{testidPrefix}-tabbar"
>
  {#each items as item, i}
    <button
      role="tab"
      aria-selected={item.key === activeTab}
      tabindex={item.key === activeTab ? 0 : -1}
      class={cn(
        "rounded-md px-3 py-1.5 text-sm font-medium transition-all",
        item.key === activeTab
          ? "bg-background text-foreground shadow-sm"
          : "text-muted-foreground hover:text-foreground",
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
