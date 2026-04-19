<script lang="ts">
  import { ChevronRight, MoreHorizontal } from "lucide-svelte";
  import { cn } from "$lib/utils";
  import { Popover } from "$lib/components/ui/popover";
  import type { BreadcrumbItem } from "./types";

  interface Props {
    items: BreadcrumbItem[];
    class?: string;
  }

  let { items, class: className = "" }: Props = $props();

  let mobilePopoverOpen = $state(false);

  const collapsible = $derived(items.length > 3);
  const firstItem = $derived(items[0]);
  const lastItem = $derived(items[items.length - 1]);
  const middleItems = $derived(items.slice(1, -1));
  const firstIcon = $derived(firstItem?.icon);
  const lastIcon = $derived(lastItem?.icon);

  function handleClick(event: MouseEvent, item: BreadcrumbItem, isLast: boolean) {
    if (isLast || !item.href) {
      event.preventDefault();
    }
  }
</script>

<nav aria-label="Breadcrumb" class={cn("text-sm", className)} data-testid="breadcrumbs">
  <!-- Desktop / non-collapsible: show every item -->
  <ol class="flex items-center gap-1.5 {collapsible ? 'hidden md:flex' : 'flex'}">
    {#each items as item, i (i + ":" + item.label)}
      {@const isLast = i === items.length - 1}
      {@const Icon = item.icon}
      <li class="flex items-center gap-1.5">
        {#if isLast}
          <span
            aria-current="page"
            class="inline-flex items-center gap-1 text-foreground font-medium"
          >
            {#if Icon}
              <Icon size={14} strokeWidth={1.75} aria-hidden="true" />
            {/if}
            <span>{item.label}</span>
          </span>
        {:else if item.href}
          <a
            href={item.href}
            class="inline-flex items-center gap-1 text-muted-foreground transition-colors hover:text-foreground"
            onclick={(e) => handleClick(e, item, false)}
          >
            {#if Icon}
              <Icon size={14} strokeWidth={1.75} aria-hidden="true" />
            {/if}
            <span>{item.label}</span>
          </a>
        {:else}
          <span class="inline-flex items-center gap-1 text-muted-foreground">
            {#if Icon}
              <Icon size={14} strokeWidth={1.75} aria-hidden="true" />
            {/if}
            <span>{item.label}</span>
          </span>
        {/if}
        {#if !isLast}
          <ChevronRight size={14} strokeWidth={1.75} class="text-muted-foreground/60" aria-hidden="true" />
        {/if}
      </li>
    {/each}
  </ol>

  <!-- Mobile collapsed: Home … Current -->
  {#if collapsible}
    <ol class="flex items-center gap-1.5 md:hidden">
      <li class="flex items-center gap-1.5">
        {#if firstItem?.href}
          <a
            href={firstItem.href}
            class="inline-flex items-center gap-1 text-muted-foreground transition-colors hover:text-foreground"
          >
            {#if firstIcon}
              {@const FirstIcon = firstIcon}
              <FirstIcon size={14} strokeWidth={1.75} aria-hidden="true" />
            {/if}
            <span>{firstItem.label}</span>
          </a>
        {:else}
          <span class="inline-flex items-center gap-1 text-muted-foreground">
            {#if firstIcon}
              {@const FirstIcon = firstIcon}
              <FirstIcon size={14} strokeWidth={1.75} aria-hidden="true" />
            {/if}
            <span>{firstItem?.label}</span>
          </span>
        {/if}
        <ChevronRight size={14} strokeWidth={1.75} class="text-muted-foreground/60" aria-hidden="true" />
      </li>

      <li class="flex items-center gap-1.5">
        <Popover
          bind:open={mobilePopoverOpen}
          side="bottom"
          align="start"
          class="min-w-[180px] p-1"
        >
          {#snippet trigger()}
            <button
              type="button"
              aria-label="Show collapsed breadcrumb items"
              aria-haspopup="menu"
              aria-expanded={mobilePopoverOpen}
              class="inline-flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              <MoreHorizontal size={14} strokeWidth={1.75} aria-hidden="true" />
            </button>
          {/snippet}
          {#snippet content()}
            <ul class="flex flex-col gap-0.5" role="menu">
              {#each middleItems as mi, idx (idx + ":" + mi.label)}
                {@const MiIcon = mi.icon}
                <li role="none">
                  {#if mi.href}
                    <a
                      href={mi.href}
                      role="menuitem"
                      class="flex items-center gap-2 rounded-md px-2 py-1.5 text-sm text-foreground transition-colors hover:bg-muted"
                      onclick={() => (mobilePopoverOpen = false)}
                    >
                      {#if MiIcon}
                        <MiIcon size={14} strokeWidth={1.75} aria-hidden="true" />
                      {/if}
                      <span>{mi.label}</span>
                    </a>
                  {:else}
                    <span
                      role="menuitem"
                      aria-disabled="true"
                      class="flex items-center gap-2 rounded-md px-2 py-1.5 text-sm text-muted-foreground"
                    >
                      {#if MiIcon}
                        <MiIcon size={14} strokeWidth={1.75} aria-hidden="true" />
                      {/if}
                      <span>{mi.label}</span>
                    </span>
                  {/if}
                </li>
              {/each}
            </ul>
          {/snippet}
        </Popover>
        <ChevronRight size={14} strokeWidth={1.75} class="text-muted-foreground/60" aria-hidden="true" />
      </li>

      <li class="flex items-center gap-1.5">
        <span
          aria-current="page"
          class="inline-flex items-center gap-1 text-foreground font-medium truncate max-w-[12rem]"
        >
          {#if lastIcon}
            {@const LastIcon = lastIcon}
            <LastIcon size={14} strokeWidth={1.75} aria-hidden="true" />
          {/if}
          <span class="truncate">{lastItem?.label}</span>
        </span>
      </li>
    </ol>
  {/if}
</nav>
