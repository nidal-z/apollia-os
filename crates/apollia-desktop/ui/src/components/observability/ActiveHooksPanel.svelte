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
   * Under the registry, the decisions the PreToolUse hooks actually took. The
   * bridge pushes them on the `hook-decision` Tauri channel and nothing is
   * persisted, so this list is what the current session produced and nothing
   * more: it starts empty on every visit and keeps the last `MAX_DECISIONS`.
   *
   * Builder-only surface: it exposes wire event names, argv and timeouts.
   */
  import { onMount, onDestroy } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { t } from "svelte-i18n";
  import { Webhook } from "lucide-svelte";
  import { Card } from "$lib/components/ui/card";
  import { Badge } from "$lib/components/ui/badge";
  import { ErrorBanner } from "$lib/components/operator";
  import { getActiveHooks, type ActiveHook } from "$lib/ipc/observability";

  /** A PreToolUse decision as the bridge pushes it. */
  interface HookDecision {
    run_id: string;
    session_id: string;
    tool_name: string;
    decision: string;
    rewritten_args: string | null;
  }

  /** Kept in memory only, so the list is bounded rather than unbounded. */
  const MAX_DECISIONS = 50;

  let hooks = $state<ActiveHook[]>([]);
  let loading = $state(true);
  let errorMessage = $state<string | null>(null);
  let decisions = $state<HookDecision[]>([]);
  let unlisten: UnlistenFn | undefined;

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

  function decisionTone(decision: string): "success" | "danger" | "neutral" {
    if (decision === "allow") return "success";
    if (decision === "deny") return "danger";
    return "neutral";
  }

  onMount(() => {
    void load();
    void listen<HookDecision>("hook-decision", (event) => {
      decisions = [event.payload, ...decisions].slice(0, MAX_DECISIONS);
    }).then((fn) => {
      unlisten = fn;
    });
  });

  onDestroy(() => {
    unlisten?.();
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
                class="rounded-full border border-border/40 glass-inset px-2 py-px font-mono text-caption text-muted-foreground"
              >
                {event}
              </span>
            {/each}
          </div>
        </div>
      {/each}
    </Card>
  {/if}

  <div class="flex flex-wrap items-baseline gap-x-3 gap-y-1 pt-2">
    <h3 class="m-0 text-body-sm font-semibold text-foreground">
      {$t("observability.hook_decisions_title")}
    </h3>
    <span class="section-meta tabular-nums" data-testid="hook-decisions-count">
      {$t("observability.hook_decisions_count", { values: { n: decisions.length } })}
    </span>
    <p class="w-full text-caption text-muted-foreground">
      {$t("observability.hook_decisions_subtitle")}
    </p>
  </div>

  {#if decisions.length === 0}
    <Card class="flex flex-col items-center justify-center py-8" data-testid="hook-decisions-empty">
      <p class="text-body-sm text-muted-foreground">
        {$t("observability.hook_decisions_empty")}
      </p>
    </Card>
  {:else}
    <Card class="divide-y divide-border/30 overflow-hidden" data-testid="hook-decisions-list">
      {#each decisions as decision (decision.run_id + decision.tool_name + decision.decision)}
        <div class="flex flex-wrap items-center gap-2 px-4 py-2" data-testid="hook-decision-row">
          <Badge variant={decisionTone(decision.decision)} size="sm">{decision.decision}</Badge>
          <code class="min-w-0 flex-1 break-all font-mono text-code-sm text-foreground/85">
            {decision.tool_name}
          </code>
          <span class="font-mono text-caption text-muted-foreground">{decision.run_id}</span>
        </div>
      {/each}
    </Card>
  {/if}
</div>
