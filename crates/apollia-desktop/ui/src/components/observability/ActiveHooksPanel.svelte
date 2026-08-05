<script lang="ts">
  /**
   * `ActiveHooksPanel` - the lifecycle hook handlers registered at startup.
   *
   * Reads `get_active_hooks`. The registry is built once from the configuration
   * and never mutates afterwards (no dynamic registration, no hot reload), so
   * this is a startup fact rather than a live feed: one read is enough.
   *
   * A failed read is the exception. It says nothing about the registry, only
   * that the call did not go through, and without a retry the panel would keep
   * showing that dead state until the operator leaves the tab. The retry reads
   * again and the panel switches to the registry as soon as one call succeeds.
   *
   * Builder-only surface: it exposes wire event names, argv and timeouts.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Webhook } from "lucide-svelte";
  import { Card } from "$lib/components/ui/card";
  import { Badge } from "$lib/components/ui/badge";
  import { ErrorBanner } from "$lib/components/operator";
  import { getActiveHooks, type ActiveHook } from "$lib/ipc/observability";

  let hooks = $state<ActiveHook[]>([]);
  let loading = $state(true);
  let errorMessage = $state<string | null>(null);

  async function load(): Promise<void> {
    loading = true;
    try {
      hooks = await getActiveHooks();
      errorMessage = null;
    } catch (err: unknown) {
      hooks = [];
      errorMessage = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void load();
  });
</script>

<div class="space-y-4" data-testid="active-hooks-panel">
  <div class="flex flex-wrap items-baseline gap-x-3 gap-y-1">
    <h3 class="m-0 text-body-sm font-semibold text-foreground">
      {$t("observability.hooks_title")}
    </h3>
    {#if !loading && errorMessage === null}
      <span class="section-meta tabular-nums" data-testid="active-hooks-count">
        {$t("observability.hooks_count", { values: { n: hooks.length } })}
      </span>
    {/if}
    <p class="w-full text-caption text-muted-foreground">
      {$t("observability.hooks_subtitle")}
    </p>
  </div>

  {#if loading}
    <p class="text-body-sm text-muted-foreground">{$t("common.loading")}</p>
  {:else if errorMessage !== null}
    <ErrorBanner
      message={$t("observability.hooks_load_error", { values: { message: errorMessage } })}
      onretry={() => void load()}
      retryLabel={$t("common.retry")}
      data-testid="active-hooks-error"
    />
  {:else if hooks.length === 0}
    <Card class="flex flex-col items-center justify-center py-12" data-testid="active-hooks-empty">
      <div class="mb-3 rounded-full glass-inset p-3">
        <Webhook class="h-6 w-6 text-muted-foreground/60" aria-hidden="true" />
      </div>
      <p class="text-body-sm text-muted-foreground">
        {$t("observability.hooks_registry_empty")}
      </p>
    </Card>
  {:else}
    <Card class="divide-y divide-border/30 overflow-hidden">
      {#each hooks as hook (hook.id)}
        <div class="px-4 py-3" data-testid="active-hook-{hook.id}">
          <div class="flex flex-wrap items-center gap-2">
            <Badge variant="neutral" size="sm">{hook.type}</Badge>
            <code class="min-w-0 flex-1 break-all font-mono text-code-sm text-foreground/85">
              {hook.target}
            </code>
            <span class="text-caption tabular-nums text-muted-foreground">
              {$t("observability.hooks_col_timeout")}
              <span class="ml-1 text-foreground/80">{hook.timeout_ms}ms</span>
            </span>
          </div>

          <div class="mt-1.5 flex flex-wrap items-center gap-1.5">
            <span class="section-meta">{$t("observability.hooks_col_events")}</span>
            {#each hook.events as event (event)}
              <span
                class="rounded-full border border-border/40 glass-inset px-2 py-[1px] font-mono text-caption text-muted-foreground"
              >
                {event}
              </span>
            {/each}
          </div>
        </div>
      {/each}
    </Card>
  {/if}
</div>
