<script lang="ts">
  /**
   * Topbar breadcrumb - V3 Operator format: "Apollia / [page]".
   *
   * The "Apollia" prefix is static and clickable (routes to home for the
   * current UI mode). A status dot is rendered next to it to surface the
   * combined runtime + LLM availability:
   *   - green: runtime heartbeat is healthy AND at least one LLM backend is enabled
   *   - amber: runtime is healthy but no LLM backend is connected
   *   - red:   runtime heartbeat is missing or reconnecting
   * The trail derives from `currentRoute` via `routeMeta`.
   *
   * Promoted from `components/layout/Breadcrumb.svelte`. Status dot shadows
   * are now driven by the `--shadow-status-*` tokens in app.css.
   */
  import { t } from "svelte-i18n";
  import { currentRoute, navigateTo } from "$lib/stores/navigation";
  import { buildTrail, routeMeta, homeRouteFor } from "$lib/navigation/routeMeta";
  import { uiMode } from "$lib/stores/mode";
  import { runtimeHealth } from "$lib/stores/runtimeHealth";
  import { readyLlmBackends } from "$lib/stores/llm";

  const trail = $derived(buildTrail($currentRoute));

  type StatusLevel = "ok" | "warn" | "down";
  const status = $derived.by<{ level: StatusLevel; key: string }>(() => {
    if ($runtimeHealth.status !== "connected") {
      return { level: "down", key: "topbar.status.runtime_down" };
    }
    if ($readyLlmBackends.length === 0) {
      return { level: "warn", key: "topbar.status.no_llm" };
    }
    return { level: "ok", key: "topbar.status.ok" };
  });

  const dotClass = $derived(
    status.level === "ok"
      ? "bg-emerald-500 shadow-status-ok"
      : status.level === "warn"
        ? "bg-amber-500 shadow-status-warn"
        : "bg-rose-500 shadow-status-error animate-pulse",
  );

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
    class="inline-flex items-center gap-1.5 text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 rounded"
    onclick={goHome}
  >
    <span
      class="inline-block h-1.5 w-1.5 rounded-full {dotClass}"
      role="status"
      aria-label={$t(status.key)}
      title={$t(status.key)}
      data-testid="topbar-status-dot"
      data-status={status.level}
    ></span>
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
