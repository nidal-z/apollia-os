<script lang="ts">
  /**
   * `ExecutionTrace` — vue conversation-like d'une exécution agent
   * (ADR-088, Lot 3).
   *
   * Source unique : table `runtime_events` exposée via la commande Tauri
   * `get_task_trace` (replay) et le bus `"trace-event"` (live, Lot 4).
   *
   * Trois contextes d'embedding :
   * - `task` : dans `TaskDetail.svelte`, remplace `TaskTimeline`.
   * - `chat` : dans `ChatConversation`, entre les messages.
   * - `standalone` : page Observability dédiée.
   *
   * Deux skins :
   * - `operator` : sémantique épurée (descriptions, pas de JSON).
   * - `builder`  : tout est rendu (args bruts, model, tokens, retries).
   *
   * Le composant pair les `tool_call_started` avec leur `tool_call_completed`
   * companion (via `parent_event_id`) avant de rendre, pour éviter d'afficher
   * deux rangées par tool call.
   */
  import { onDestroy, onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { AlertCircle, Loader2 } from "lucide-svelte";
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

  interface Props {
    taskId: string;
    /** "task" / "chat" / "standalone" — détermine le rendu container. */
    context?: "task" | "chat" | "standalone";
    /** Override du `$uiMode` global. Sinon hérite. */
    skin?: UIMode | undefined;
    /** "live" = abonné SSE, "replay" = chargement statique uniquement. */
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
   * Pair started↔completed events.
   *
   * Pour chaque `tool_call_started`, on cherche le `tool_call_completed`
   * dont `parent_event_id === started.event_id`. La paire est rendue
   * comme un seul `TraceEventCard` (le started avec son completion en prop).
   * Les `completed` non-orphelins ne sont pas rendus seuls.
   */
  function pairEvents(events: RuntimeEventDto[]): {
    event: RuntimeEventDto;
    completion: RuntimeEventDto | null;
  }[] {
    const completedByParent = new Map<string, RuntimeEventDto>();
    for (const e of events) {
      if (e.kind === "tool_call_completed" && e.parentEventId) {
        completedByParent.set(e.parentEventId, e);
      }
    }
    const out: { event: RuntimeEventDto; completion: RuntimeEventDto | null }[] = [];
    for (const e of events) {
      if (e.kind === "tool_call_completed" && e.parentEventId && completedByParent.has(e.parentEventId)) {
        // Ne pas rendre seul — déjà associé via le started.
        continue;
      }
      if (e.kind === "tool_call_started") {
        out.push({ event: e, completion: completedByParent.get(e.eventId) ?? null });
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
    // Ne pas clear — la trace peut être réaffichée si l'utilisateur revient.
    // `clearTrace(taskId)` sera appelé manuellement par le parent en cas de
    // pression mémoire. Pas un cas réel pour le MVP.
    void clearTrace; // référence pour qu'eslint ne warn pas sur l'import.
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
    <div class="flex items-center gap-1.5 px-2 text-[10.5px] text-muted-foreground">
      <span
        class="inline-block h-1.5 w-1.5 rounded-full bg-success animate-pulse"
        aria-hidden="true"
      ></span>
      <span>{$t("trace.live_indicator")}</span>
    </div>
  {/if}

  {#if $traceState.error}
    <div class="flex items-start gap-2 rounded-lg border border-destructive/20 bg-destructive/5 px-2.5 py-2 text-[12px] text-destructive">
      <AlertCircle size={13} class="shrink-0 mt-0.5" />
      <span>{$t("trace.error_load", { values: { error: $traceState.error } })}</span>
    </div>
  {/if}

  {#if $traceState.events.length === 0 && $traceState.loading}
    <div class="flex items-center gap-2 px-2.5 py-3 text-[12px] text-muted-foreground">
      <Loader2 size={13} class="animate-spin" />
      <span>{$t("trace.loading")}</span>
    </div>
  {:else if $traceState.events.length === 0 && !$traceState.loading}
    <div class="px-2.5 py-3 text-center text-[12px] text-muted-foreground">
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
      <button
        type="button"
        onclick={loadMore}
        disabled={$traceState.loading}
        class="self-center mt-1.5 px-3 py-1 text-[11px] text-primary hover:underline disabled:opacity-50"
      >
        {#if $traceState.loading}
          <Loader2 size={11} class="inline animate-spin" />
        {/if}
        {$t("trace.load_more")}
      </button>
    {/if}
  {/if}
</div>
