<script lang="ts" module>
  import type { HookHandler } from "$lib/ipc/hooks";

  /** Normalizes an unknown thrown value into a display string. */
  export function hookErrorMessage(err: unknown): string {
    return err instanceof Error ? err.message : String(err);
  }

  /** True when a handler subscribes to the blocking PreToolUse event. */
  export function handlerHasPreToolUse(handler: HookHandler): boolean {
    return handler.events.includes("pre_tool_use");
  }
</script>

<script lang="ts">
  /**
   * `HooksView` - read-only list of the registered lifecycle hooks plus the
   * live PreToolUse decision log. Builder-only surface.
   *
   * The handler list is loaded once at mount; configuration stays in the agent
   * files, never edited from the UI.
   */
  import { t } from "svelte-i18n";
  import { Settings2 } from "lucide-svelte";
  import { getActiveHooks } from "$lib/ipc/hooks";
  import HookDecisionLog from "./HookDecisionLog.svelte";

  interface Props {
    /** Restrict the decision log to one session; otherwise logs every run. */
    sessionId?: string | undefined;
  }

  let { sessionId = undefined }: Props = $props();

  let hooks = $state<HookHandler[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);

  const isEmpty = $derived(!loading && !error && hooks.length === 0);

  $effect(() => {
    loading = true;
    error = null;
    getActiveHooks()
      .then((h) => {
        hooks = h;
      })
      .catch((err: unknown) => {
        error = hookErrorMessage(err);
      })
      .finally(() => {
        loading = false;
      });
  });
</script>

<section class="flex flex-col gap-4" data-testid="hooks-view">
  <div class="flex flex-col gap-2">
    <h3 class="flex items-center gap-2 text-[13px] font-semibold">
      <Settings2 class="h-4 w-4 text-primary" />
      {$t("observability.hooks_title")}
    </h3>

    {#if loading}
      <p class="text-[12px] text-muted-foreground">{$t("common.loading")}</p>
    {:else if error}
      <p class="text-[12px] text-destructive" data-testid="hooks-load-error">
        {$t("observability.hooks_load_error", { values: { message: error } })}
      </p>
    {:else if isEmpty}
      <p class="text-[12px] text-muted-foreground" data-testid="hooks-empty-state">
        {$t("observability.hooks_empty")}
      </p>
    {:else}
      <ul class="flex flex-col gap-1.5" data-testid="hooks-list">
        {#each hooks as hook (hook.id)}
          <li
            class="flex items-center justify-between gap-3 rounded-md border border-border/40 bg-muted/30 px-3 py-2"
            data-testid={handlerHasPreToolUse(hook) ? "hook-item-pre-tool-use" : "hook-item"}
          >
            <div class="flex min-w-0 flex-col">
              <code class="truncate font-mono text-[12px] text-foreground">{hook.target}</code>
              <span class="text-[10px] uppercase tracking-wide text-muted-foreground">
                {hook.type}
              </span>
            </div>
            <div class="flex flex-wrap justify-end gap-1">
              {#each hook.events as event (event)}
                <span class="rounded glass-inset px-1.5 py-0.5 text-[10px] text-muted-foreground">
                  {event}
                </span>
              {/each}
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <HookDecisionLog {sessionId} />
</section>
