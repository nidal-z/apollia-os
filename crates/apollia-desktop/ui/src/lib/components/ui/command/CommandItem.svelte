<script lang="ts">
  import { cn } from "$lib/utils";
  import { ArrowRight } from "lucide-svelte";
  import type { CommandItem } from "$lib/stores/commandPalette";

  interface Props {
    item: CommandItem;
    active: boolean;
    id: string;
    onselect: () => void;
    onhover: () => void;
  }

  let { item, active, id, onselect, onhover }: Props = $props();

  const Icon = $derived(item.icon);
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  {id}
  role="option"
  aria-selected={active}
  tabindex="-1"
  class={cn(
    "flex items-center gap-2.5 rounded-md px-2.5 py-2 text-sm text-card-foreground cursor-default select-none",
    active && "bg-muted",
  )}
  onclick={onselect}
  onmousemove={onhover}
  data-testid="command-item"
>
  {#if Icon}
    {@const IconComponent = Icon}
    <span class="text-muted-foreground" aria-hidden="true">
      <IconComponent size={14} strokeWidth={1.75} />
    </span>
  {/if}
  <span class="flex min-w-0 flex-1 flex-col">
    <span class="truncate">{item.label}</span>
    {#if item.description}
      <span class="truncate text-xs text-muted-foreground">{item.description}</span>
    {/if}
  </span>
  {#if item.shortcut}
    <span class="flex items-center gap-1" aria-hidden="true">
      {#each item.shortcut as key}
        <kbd
          class="inline-flex items-center border-b border-border bg-muted px-1.5 py-[1px] text-[10px] font-mono text-muted-foreground rounded-sm"
        >
          {key}
        </kbd>
      {/each}
    </span>
  {/if}
  {#if active}
    <span class="text-muted-foreground" aria-hidden="true">
      <ArrowRight size={12} strokeWidth={1.75} />
    </span>
  {/if}
</div>
