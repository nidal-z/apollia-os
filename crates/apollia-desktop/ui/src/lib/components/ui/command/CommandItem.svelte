<script lang="ts">
  /**
   * Palette result row. The active row carries the signature gradient (inset
   * rail + diagonal tint), its icon turns `--primary`, and the trailing arrow
   * slides in. The matched query fragment is highlighted when it appears as a
   * contiguous substring of the label.
   */
  import { cn } from "$lib/utils";
  import { ArrowRight } from "lucide-svelte";
  import type { CommandItem } from "$lib/stores/commandPalette";
  import Keycap from "./Keycap.svelte";

  interface Props {
    item: CommandItem;
    active: boolean;
    id: string;
    query?: string;
    onselect: () => void;
    onhover: () => void;
  }

  let { item, active, id, query = "", onselect, onhover }: Props = $props();

  const Icon = $derived(item.icon);

  interface LabelParts {
    before: string;
    match: string;
    after: string;
  }

  const parts = $derived.by<LabelParts>(() => {
    const q = query.trim();
    if (!q) return { before: item.label, match: "", after: "" };
    const i = item.label.toLowerCase().indexOf(q.toLowerCase());
    if (i === -1) return { before: item.label, match: "", after: "" };
    return {
      before: item.label.slice(0, i),
      match: item.label.slice(i, i + q.length),
      after: item.label.slice(i + q.length),
    };
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  {id}
  role="option"
  aria-selected={active}
  tabindex="-1"
  class={cn("row", active && "active")}
  onclick={onselect}
  onmousemove={onhover}
  data-testid={`command-item-${item.id}`}
>
  {#if Icon}
    {@const IconComponent = Icon}
    <span class="ico" aria-hidden="true">
      <IconComponent size={15} strokeWidth={1.75} />
    </span>
  {/if}
  <span class="flex min-w-0 flex-1 flex-col">
    <span class="label truncate"
      >{parts.before}{#if parts.match}<mark class="hit">{parts.match}</mark
        >{/if}{parts.after}</span
    >
    {#if item.description}
      <span class="desc truncate">{item.description}</span>
    {/if}
  </span>
  {#if item.shortcut}
    <span class="flex items-center gap-1" aria-hidden="true">
      {#each item.shortcut as key (key)}
        <Keycap label={key} />
      {/each}
    </span>
  {/if}
  <span class="arrow" aria-hidden="true">
    <ArrowRight size={13} strokeWidth={2} />
  </span>
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 0.625rem;
    padding: 0.5rem 0.625rem;
    border-radius: calc(var(--radius) - 0.25rem);
    cursor: default;
    user-select: none;
    position: relative;
    color: hsl(var(--card-foreground));
    transition: background var(--motion-fast) var(--ease-standard);
  }

  .row:hover {
    background: hsl(var(--muted) / 0.6);
  }

  .ico {
    flex: none;
    display: grid;
    place-items: center;
    color: hsl(var(--muted-foreground));
  }

  .label {
    font-size: var(--text-body-sm);
  }

  .desc {
    font-size: var(--text-caption-lg);
    color: hsl(var(--muted-foreground));
  }

  .arrow {
    flex: none;
    color: hsl(var(--primary));
    opacity: 0;
    transform: translateX(-0.125rem);
    transition:
      opacity var(--motion-fast),
      transform var(--motion-fast) var(--ease-apple);
  }

  .hit {
    background: hsl(var(--primary) / 0.22);
    color: hsl(var(--card-foreground));
    border-radius: 0.1875rem;
    padding: 0 0.0625rem;
  }

  .row.active {
    background: linear-gradient(
      90deg,
      hsl(var(--grad-a) / 0.16),
      hsl(var(--grad-b) / 0.1)
    );
    box-shadow:
      inset 0.125rem 0 0 hsl(var(--grad-a)),
      inset 0 0 0 1px hsl(var(--grad-b) / 0.18);
  }

  .row.active .label {
    font-weight: 550;
  }

  .row.active .ico {
    color: hsl(var(--primary));
  }

  .row.active .arrow {
    opacity: 1;
    transform: none;
  }

  @media (prefers-reduced-motion: reduce) {
    .row,
    .arrow {
      transition: none;
    }
  }
</style>
