<script lang="ts" module>
  import type { HookDecisionKind, HookDecisionPayload } from "$lib/ipc/hooks";

  /** Max decisions kept in the in-memory log (newest first). */
  export const DECISION_LOG_CAP = 100;

  /** Lifecycle event the log covers (only PreToolUse decisions are emitted). */
  export const HOOK_EVENT_LABEL = "PreToolUse";

  /** A received decision augmented with a stable id and a receipt timestamp. */
  export interface LoggedDecision extends HookDecisionPayload {
    id: number;
    at: string;
  }

  /** Token color class for a decision kind. */
  export function decisionClass(kind: HookDecisionKind): string {
    if (kind === "deny") return "text-destructive";
    if (kind === "rewrite") return "text-warning";
    return "text-success";
  }

  /** i18n key for a decision kind label. */
  export function decisionLabelKey(kind: HookDecisionKind): string {
    return `observability.hooks_decision_${kind}`;
  }

  /**
   * Prepends `entry` to `list` (newest first) and caps the length.
   *
   * Returns a new array so Svelte reactivity fires; the original is untouched.
   */
  export function appendDecision(
    list: LoggedDecision[],
    entry: LoggedDecision,
    cap: number = DECISION_LOG_CAP,
  ): LoggedDecision[] {
    return [entry, ...list].slice(0, cap);
  }
</script>

<script lang="ts">
  /**
   * `HookDecisionLog` - live log of PreToolUse decisions.
   *
   * Decisions are not persisted: the log accumulates the dedicated
   * `"hook-decision"` Tauri event in memory, newest first, optionally filtered
   * to one session. The listener is always unsubscribed on teardown.
   */
  import { t } from "svelte-i18n";
  import { listen } from "@tauri-apps/api/event";

  interface Props {
    /** Restrict the log to one session; otherwise records every run. */
    sessionId?: string | undefined;
  }

  let { sessionId = undefined }: Props = $props();

  let decisions = $state<LoggedDecision[]>([]);
  let nextId = 0;

  const isEmpty = $derived(decisions.length === 0);

  $effect(() => {
    const target = sessionId;
    let unlisten: (() => void) | null = null;
    listen<HookDecisionPayload>("hook-decision", (event) => {
      const payload = event.payload;
      if (target !== undefined && payload.session_id !== target) return;
      const entry: LoggedDecision = {
        ...payload,
        id: nextId++,
        at: new Date().toLocaleTimeString(),
      };
      decisions = appendDecision(decisions, entry);
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => {
        // A listener failure leaves the log empty; the hooks list still renders.
      });
    return () => {
      unlisten?.();
    };
  });

  let expanded = $state<Set<number>>(new Set());

  function toggle(id: number) {
    const next = new Set(expanded);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    expanded = next;
  }
</script>

<div class="flex flex-col gap-2">
  <h4 class="text-[12px] font-semibold text-muted-foreground">
    {$t("observability.hooks_decision_log_title")}
  </h4>

  {#if isEmpty}
    <p class="text-[12px] text-muted-foreground" data-testid="hook-decision-log-empty">
      {$t("observability.hooks_no_decisions")}
    </p>
  {:else}
    <table class="w-full text-[12px]" data-testid="hook-decision-log">
      <thead>
        <tr class="border-b border-border/40 text-left text-muted-foreground">
          <th class="px-2 py-1 font-medium">{$t("observability.hooks_col_timestamp")}</th>
          <th class="px-2 py-1 font-medium">{$t("observability.hooks_col_hook")}</th>
          <th class="px-2 py-1 font-medium">{$t("observability.hooks_col_tool")}</th>
          <th class="px-2 py-1 font-medium">{$t("observability.hooks_col_decision")}</th>
        </tr>
      </thead>
      <tbody>
        {#each decisions as d (d.id)}
          <tr
            class="border-b border-border/20 last:border-0"
            class:cursor-pointer={d.decision === "rewrite"}
            onclick={() => d.decision === "rewrite" && toggle(d.id)}
            data-testid={`hook-decision-${d.id}`}
          >
            <td class="px-2 py-1 tabular-nums text-muted-foreground">{d.at}</td>
            <td class="px-2 py-1 text-muted-foreground">{HOOK_EVENT_LABEL}</td>
            <td class="px-2 py-1">
              <code class="font-mono text-foreground">{d.tool_name}</code>
            </td>
            <td class="px-2 py-1">
              {#if d.decision === "rewrite"}
                <span class="rounded bg-warning/15 px-1.5 py-0.5 text-warning">
                  {$t(decisionLabelKey(d.decision))}
                </span>
              {:else}
                <span class={decisionClass(d.decision)}>
                  {$t(decisionLabelKey(d.decision))}
                </span>
              {/if}
            </td>
          </tr>
          {#if d.decision === "rewrite" && d.rewritten_args && expanded.has(d.id)}
            <tr>
              <td colspan="4" class="px-2 pb-2">
                <pre class="overflow-x-auto rounded glass-surface p-2 font-mono text-[11px]">{d.rewritten_args}</pre>
              </td>
            </tr>
          {/if}
        {/each}
      </tbody>
    </table>
  {/if}
</div>
