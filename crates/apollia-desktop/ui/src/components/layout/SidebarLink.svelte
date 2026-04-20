<script lang="ts">
  /**
   * SidebarLink — single nav entry (US-SP42-079).
   *
   * Bounded props so Svelte's reactive scope only invalidates this link
   * when its own inputs change. The parent sidebar no longer re-renders
   * every button on an unrelated store change.
   */
  import type { ComponentType, Snippet } from "svelte";

  interface Props {
    icon: ComponentType;
    label: string;
    active: boolean;
    collapsed: boolean;
    testId: string;
    onSelect: () => void;
    /** Optional badge slot — rendered only when labels are visible. */
    badge?: Snippet;
    /** Indicator rendered at the top-right when collapsed (e.g. pulsing dot). */
    indicator?: Snippet;
  }

  let {
    icon: Icon,
    label,
    active,
    collapsed,
    testId,
    onSelect,
    badge,
    indicator,
  }: Props = $props();
</script>

<button
  type="button"
  class="list-item-spring relative flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 {active
    ? 'bg-primary/10 text-primary ring-1 ring-primary/20'
    : 'text-muted-foreground hover:bg-primary/[0.04] hover:text-foreground'}"
  class:justify-center={collapsed}
  class:px-2={collapsed}
  data-nav-item="true"
  aria-current={active ? "page" : undefined}
  aria-label={collapsed ? label : undefined}
  data-testid={testId}
  onclick={onSelect}
  title={collapsed ? label : undefined}
>
  <Icon size={18} strokeWidth={1.75} class="shrink-0" />
  {#if collapsed && indicator}
    {@render indicator()}
  {/if}
  {#if !collapsed}
    <span class="truncate">{label}</span>
    {#if badge}
      {@render badge()}
    {/if}
  {/if}
</button>
