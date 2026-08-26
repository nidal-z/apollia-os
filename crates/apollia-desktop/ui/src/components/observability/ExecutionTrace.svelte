<script lang="ts">
  /**
   * `ExecutionTrace` - conversation-like view of one agent run.
   *
   * Single source: the `runtime_events` table, exposed through the Tauri
   * command `get_task_trace` (replay) and the `"trace-event"` bus (live).
   *
   * Three embedding contexts:
   * - `task`: inside `TaskDetail.svelte`, replacing `TaskTimeline`.
   * - `chat`: inside `ChatConversation`, between the messages.
   * - `standalone`: the dedicated Observability page.
   *
   * Two skins:
   * - `operator`: pared-down semantics (descriptions, no JSON).
   * - `builder` : everything is rendered (raw args, model, tokens, retries).
   *
   * The component pairs each `tool_call_started` with its companion
   * `tool_call_completed` (through `parent_event_id`) before rendering, so a
   * tool call does not show two rows.
   */
  import { onDestroy, onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { AlertCircle } from "lucide-svelte";
  import type { RuntimeEventDto } from "$lib/trace";
  import {
    clearTrace,
    loadTrace,
    subscribeTraceLive,
    traceFor,
    unsubscribeTraceLive,
  } from "$lib/stores/trace";
  import { uiMode, type UIMode } from "$lib/stores/mode";
  import TraceEventCard from "./TraceEventCard.svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    taskId: string;
    /** "task" / "chat" / "standalone" - decides the container rendering. */
    context?: "task" | "chat" | "standalone";
    /** Override of the global `$uiMode`. Inherited otherwise. */
    skin?: UIMode | undefined;
    /** "live" = subscribed to SSE, "replay" = static loading only. */
    mode?: "live" | "replay";
  }

  let {
    taskId,
    context = "task",
    skin = undefined,
    mode = "live",
  }: Props = $props();

  const traceState = $derived(traceFor(taskId));
  const effectiveSkin = $derived<UIMode>(skin ?? $uiMode);

  /**
   * Pair started ↔ completed/denied events.
   *
   * For every `tool_call_started`, its companion `tool_call_completed` OR
   * `tool_call_denied` is looked up by `parent_event_id === started.event_id`.
   * The pair is rendered as a single `TraceEventCard`; a lone started stays
   * on a spinner only while NO companion has arrived yet.
   *
   *
   * The original defect: only `completed` was paired. When a tool was
   * `denied` (sandbox, permission, manifest), the started stayed on the
   * "Reading..." spinner forever although the closing event existed.
   */
  function pairEvents(events: RuntimeEventDto[]): {
    event: RuntimeEventDto;
    completion: RuntimeEventDto | null;
  }[] {
    const closersByParent = new Map<string, RuntimeEventDto>();
    for (const e of events) {
      if (
        (e.kind === "tool_call_completed" || e.kind === "tool_call_denied") &&
        e.parentEventId
      ) {
        closersByParent.set(e.parentEventId, e);
      }
    }
    const out: { event: RuntimeEventDto; completion: RuntimeEventDto | null }[] = [];
    for (const e of events) {
      if (
        (e.kind === "tool_call_completed" || e.kind === "tool_call_denied") &&
        e.parentEventId &&
        closersByParent.has(e.parentEventId)
      ) {
        // Do not render it alone - it is already attached to the started.
        continue;
      }
      if (e.kind === "tool_call_started") {
        out.push({ event: e, completion: closersByParent.get(e.eventId) ?? null });
      } else {
        out.push({ event: e, completion: null });
      }
    }
    return out;
  }

  const paired = $derived(pairEvents($traceState.events));

  // ── Lifecycle ────────────────────────────────────────────────────
  onMount(async () => {
    await loadTrace(taskId, { reset: true });
    if (mode === "live") {
      await subscribeTraceLive(taskId);
    }
  });

  onDestroy(() => {
    unsubscribeTraceLive(taskId);
    // Do not clear - the trace can be shown again if the user comes back.
    // `clearTrace(taskId)` is called by the parent by hand under memory
    // pressure. Not a real case for the MVP.
    void clearTrace; // referenced so eslint does not warn about the import.
  });

  async function loadMore() {
    if (!$traceState.nextCursor) return;
    await loadTrace(taskId, { since: $traceState.nextCursor });
  }
</script>

<div
  class="flex flex-col gap-1.5"
  data-testid="execution-trace"
  data-context={context}
  data-skin={effectiveSkin}
>
  {#if $traceState.live && context !== "standalone"}
    <div class="flex items-center gap-1.5 px-2 text-micro-lg text-muted-foreground">
      <span
        class="inline-block h-1.5 w-1.5 rounded-full bg-success animate-pulse"
        aria-hidden="true"
      ></span>
      <span>{$t("trace.live_indicator")}</span>
    </div>
  {/if}

  {#if $traceState.error}
    <div class="flex items-start gap-2 rounded-lg border border-destructive/20 bg-destructive/5 px-2.5 py-2 text-body-xs text-destructive">
      <AlertCircle size={13} class="shrink-0 mt-0.5" />
      <span>{$t("trace.error_load", { values: { error: $traceState.error } })}</span>
    </div>
  {/if}

  {#if $traceState.events.length === 0 && $traceState.loading}
    <div class="flex items-center gap-2 px-2.5 py-3 text-body-xs text-muted-foreground">
      <Spinner size={13} />
      <span>{$t("trace.loading")}</span>
    </div>
  {:else if $traceState.events.length === 0 && !$traceState.loading}
    <div class="px-2.5 py-3 text-center text-body-xs text-muted-foreground">
      {$t("trace.empty")}
    </div>
  {:else}
    {#each paired as item (item.event.eventId)}
      <TraceEventCard
        event={item.event}
        completion={item.completion}
        skin={effectiveSkin}
      />
    {/each}

    {#if $traceState.nextCursor}
      <Button variant="ghost" size="sm"
        type="button"
        onclick={loadMore}
        disabled={$traceState.loading}
        class="self-center mt-1.5 px-3 py-1 text-caption text-primary hover:underline disabled:opacity-50"
      >
        {#if $traceState.loading}
          <Spinner size={11} class="inline" />
        {/if}
        {$t("trace.load_more")}
      </Button>
    {/if}
  {/if}
</div>
