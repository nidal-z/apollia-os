<script lang="ts" module>
  /**
   * The rank of a sidebar entry: the everyday screens, the quieter second
   * rank, the technical screens of the developer group.
   */
  export type NavRank = "primary" | "secondary" | "developer";
</script>

<script lang="ts">
  /**
   * NavRow - one entry of the sidebar: its icon, its name, an optional count.
   *
   * The same entry draws as a labelled row (the column and the drawer) or as
   * an icon with a tooltip (the rail of a medium window). The rank sets how
   * much the entry weighs at rest: a primary entry reads in the foreground
   * colour, the two other ranks step back.
   */
  import type { Route } from "$lib/stores/navigation";
  import type { MessageSquare } from "lucide-svelte";

  interface Props {
    route: Route;
    Icon: typeof MessageSquare;
    label: string;
    rank: NavRank;
    active: boolean;
    /** A count shown beside the name; nothing at zero. */
    count?: number;
    /** `rail` draws the icon alone, the name in a tooltip. */
    variant?: "row" | "rail";
    /** Where the rail tooltip opens, toward the content. */
    tooltipClass?: string;
    onselect: (route: Route) => void;
  }

  let {
    route,
    Icon,
    label,
    rank,
    active,
    count = 0,
    variant = "row",
    tooltipClass = "",
    onselect,
  }: Props = $props();

  const tone = $derived.by(() => {
    if (active) return "bg-muted text-foreground";
    if (rank === "primary") return "text-foreground hover:bg-surface-2";
    return "text-muted-foreground hover:bg-surface-2 hover:text-foreground";
  });
</script>

{#if variant === "rail"}
  <div class="rail-item group relative">
    <button
      type="button"
      class="relative flex h-9 w-9 items-center justify-center rounded-lg transition-colors duration-fast {tone}"
      onclick={() => onselect(route)}
      aria-current={active ? "page" : undefined}
      aria-label={label}
      data-rank={rank}
      data-testid="nav-{route}"
    >
      {#if active}
        <span class="active-bar"></span>
      {/if}
      <Icon size={17} strokeWidth={1.5} />
      {#if count > 0}
        <span class="absolute -right-0.5 -top-0.5 min-w-3.5 rounded-full bg-primary px-1 py-px text-center text-overline font-semibold leading-none text-primary-foreground" data-testid="nav-{route}-count">{count > 9 ? "9+" : count}</span>
      {/if}
    </button>
    <!--
      The rail entry carries `aria-label` and no `title`: a native `title`
      would draw a second label, in the system chrome and after the system
      delay, over the one below.
    -->
    <span class="tooltip pointer-events-none absolute {tooltipClass} top-1/2 z-overlay -translate-y-1/2 whitespace-nowrap rounded-md bg-foreground px-2 py-1 text-caption font-medium text-background opacity-0 shadow-elev-3 transition-opacity duration-fast group-hover:opacity-100">
      {label}
    </span>
  </div>
{:else}
  <button
    type="button"
    class="flex w-full items-center gap-3 rounded-md px-3 transition-colors duration-fast {rank === 'primary' ? 'py-2 text-sm font-medium' : 'py-1.5 text-body-sm'} {tone}"
    onclick={() => onselect(route)}
    aria-current={active ? "page" : undefined}
    data-rank={rank}
    data-testid="nav-{route}"
  >
    <Icon size={rank === "primary" ? 17 : 15} strokeWidth={1.5} class="shrink-0" />
    <span class="truncate">{label}</span>
    {#if count > 0}
      <span class="ml-auto min-w-5 rounded-full border border-border bg-background px-1.5 py-0.5 text-center text-overline font-semibold leading-none text-primary" data-testid="nav-{route}-count">{count}</span>
    {/if}
  </button>
{/if}

<style>
  .rail-item {
    /* The rail seats a 36px button in a 64px column: a 14px gutter each side,
       which the active indicator crosses to reach the edge of the rail. */
    --rail-gutter: 0.875rem;
  }

  /* Active-route indicator: a 3px bar that grows in from the rail edge. The
     gradient reuses the signature `--grad-a/--grad-b` stops, so both themes
     resolve automatically. Under reduced motion the global rule in app.css
     neutralizes the entry animation. */
  .active-bar {
    position: absolute;
    left: calc(-1 * var(--rail-gutter));
    top: 0.5rem;
    bottom: 0.5rem;
    width: 3px;
    border-radius: 0 3px 3px 0;
    background: linear-gradient(180deg, hsl(var(--grad-a)), hsl(var(--grad-b)));
    transform-origin: center;
    animation: active-bar-in var(--motion-base) var(--ease-apple);
  }

  /* Navigation on the right: the bar anchors to the other edge of the rail,
     and the mirror puts its rounded side toward the content. */
  :global(.rail-right) .active-bar {
    left: auto;
    right: calc(-1 * var(--rail-gutter));
    scale: -1 1;
  }

  @keyframes active-bar-in {
    from {
      transform: scaleY(0);
      opacity: 0;
    }
    to {
      transform: scaleY(1);
      opacity: 1;
    }
  }
</style>
