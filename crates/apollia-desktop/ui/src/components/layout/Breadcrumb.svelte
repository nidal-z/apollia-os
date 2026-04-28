<script lang="ts">
  /**
   * Topbar breadcrumb — V3 Operator format: "Apollia / [page]".
   *
   * The "Apollia" prefix is static and clickable (routes to home for the
   * current UI mode). The trail derives from `currentRoute` via `routeMeta`;
   * intermediate parents (e.g. settings → permission rules) appear between
   * the prefix and the current page. The current page renders with a
   * primary-tinted icon and a `/` separator everywhere.
   */
  import { t } from "svelte-i18n";
  import { currentRoute, navigateTo } from "$lib/stores/navigation";
  import { buildTrail, routeMeta, homeRouteFor } from "$lib/navigation/routeMeta";
  import { uiMode } from "$lib/stores/mode";

  const trail = $derived(buildTrail($currentRoute));

  function goHome() {
    navigateTo(homeRouteFor($uiMode));
  }
</script>

<nav
  aria-label="Breadcrumb"
  class="flex min-w-0 items-center gap-2 text-[12.5px]"
  data-testid="topbar-breadcrumb"
>
  <button
    type="button"
    class="text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 rounded"
    onclick={goHome}
  >
    Apollia
  </button>

  {#each trail as route, i (route)}
    {@const meta = routeMeta[route]}
    {@const isLast = i === trail.length - 1}
    {@const Icon = meta.icon}
    <span class="text-muted-foreground/50" aria-hidden="true">/</span>
    {#if isLast}
      <span
        aria-current="page"
        class="inline-flex min-w-0 items-center gap-1.5 font-medium text-foreground"
      >
        {#if Icon}
          <Icon size={12} strokeWidth={1.75} class="text-primary shrink-0" aria-hidden="true" />
        {/if}
        <span class="truncate">{$t(meta.labelKey)}</span>
      </span>
    {:else}
      <button
        type="button"
        class="inline-flex items-center gap-1.5 text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 rounded"
        onclick={() => navigateTo(route)}
      >
        {#if Icon}
          <Icon size={12} strokeWidth={1.75} aria-hidden="true" />
        {/if}
        <span>{$t(meta.labelKey)}</span>
      </button>
    {/if}
  {/each}
</nav>
