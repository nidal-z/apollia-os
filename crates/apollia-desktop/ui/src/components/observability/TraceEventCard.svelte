<script lang="ts">
  /**
   * Renders a single `RuntimeEventDto` with a discriminated render path
   * per `kind`, parametrised by skin (operator vs builder).
   *
   * - **Operator skin** : prose épurée, sémantique. Pas de JSON, pas de
   *   model name, pas de tokens. Les events purement techniques
   *   (`llm_call_started`, debug logs) sont silencieux.
   * - **Builder skin** : tout est rendu. Prompts, args/output JSON
   *   bruts, model, tokens, durées, retry chains.
   *
   * Le composant délègue à `describeBashCommand` / `describeToolCall` du
   * util `bashDescriber.ts` pour la prose operator des tool calls.
   */
  import { t } from "svelte-i18n";
  import {
    AlertTriangle,
    CheckCircle2,
    Cpu,
    Info,
    Lightbulb,
    MessageSquare,
    Network,
    RefreshCw,
    ShieldOff,
    Wrench,
    XCircle,
  } from "lucide-svelte";
  import type { RuntimeEventDto } from "$lib/trace";
  import { describeToolCall } from "$lib/utils/bashDescriber";
  import { Spinner } from "$lib/components/ui/progress";

  interface Props {
    event: RuntimeEventDto;
    skin: "operator" | "builder";
    /** Optional companion `tool_call_completed` paired with a started event
     *  via `parent_event_id`. Lets the card render duration + success in
     *  one block instead of two stacked rows. */
    completion?: RuntimeEventDto | null;
  }

  let { event, skin, completion = null }: Props = $props();

  let expanded = $state(false);

  function toggle() {
    expanded = !expanded;
  }

  function safeParse(json: string | null | undefined): unknown {
    if (!json) return null;
    try {
      return JSON.parse(json);
    } catch {
      return json;
    }
  }

  /** Short-form duration for operator skin (e.g. "412 ms", "1.8 s"). */
  function fmtDuration(ms: number): string {
    if (ms < 1000) return `${ms} ms`;
    return `${(ms / 1000).toFixed(1)} s`;
  }

  /** Truncate UTF-8 string by chars (display only). */
  function truncChars(s: string, n: number): string {
    const chars = Array.from(s);
    return chars.length > n ? chars.slice(0, n).join("") + "…" : s;
  }
</script>

{#if event.kind === "agent_log"}
  {@const level = String(event.payload.level ?? "info")}
  {#if skin === "operator" && (level === "debug" || level === "info")}
    <!-- Operator silence sur info/debug : pas de bruit -->
  {:else}
    <div
      class="flex items-start gap-2 rounded-lg px-2.5 py-2 {level === 'error'
        ? 'bg-destructive/5 border border-destructive/20'
        : level === 'warn'
          ? 'bg-warning/5 border border-warning/20'
          : 'bg-surface-1/40'}"
      data-testid="trace-event"
      data-kind="agent_log"
    >
      <div class="shrink-0 mt-0.5">
        {#if level === "error"}
          <XCircle size={13} class="text-destructive" />
        {:else if level === "warn"}
          <AlertTriangle size={13} class="text-warning" />
        {:else}
          <Info size={13} class="text-muted-foreground" />
        {/if}
      </div>
      <div class="min-w-0 flex-1">
        <div class="text-body-xs text-foreground">
          {String(event.payload.message ?? "")}
        </div>
        {#if skin === "builder"}
          <div class="font-mono mt-0.5 text-caption text-muted-foreground">
            {level} · {event.agentId} · {event.ts.split("T")[1]?.slice(0, 12)}
          </div>
        {/if}
      </div>
    </div>
  {/if}

{:else if event.kind === "thought"}
  {@const text = String(event.payload.text ?? "")}
  <div class="flex items-start gap-2 rounded-lg px-2.5 py-2 bg-surface-1/30" data-testid="trace-event" data-kind="thought">
    <Lightbulb size={13} class="shrink-0 mt-0.5 text-primary/70" />
    <div class="min-w-0 flex-1">
      {#if skin === "operator"}
        <div class="text-body-xs italic text-muted-foreground">
          {truncChars(text, 200)}
        </div>
      {:else}
        <div class="text-body-xs text-foreground whitespace-pre-wrap">{text}</div>
        <div class="font-mono mt-0.5 text-caption text-muted-foreground">
          {$t("trace.step", { values: { n: event.stepNum ?? "?" } })}
        </div>
      {/if}
    </div>
  </div>

{:else if event.kind === "tool_call_started"}
  {@const toolName = String(event.payload.tool_name ?? "")}
  {@const argsJson = (event.payload.args_json ?? null) as string | null}
  {@const fallback = toolName.startsWith("a2a:")
    ? `Delegating to ${toolName.slice(4)}`
    : toolName}
  {@const operator = describeToolCall(toolName, argsJson, fallback)}
  {@const isCompleted = completion !== null}
  {@const isDenied = completion?.kind === "tool_call_denied"}
  {@const deniedPayload = (isDenied ? completion?.payload : null) as
    | { reason?: string; detail?: string | null }
    | null}
  {@const completionPayload = (!isDenied && completion ? completion.payload : null) as
    | { success?: boolean; duration_ms?: number; output_json?: string | null }
    | null}
  {@const success = completionPayload ? Boolean(completionPayload.success) : null}
  {@const durationMs = completionPayload
    ? Number(completionPayload.duration_ms ?? 0)
    : null}
  <div
    class="rounded-lg border overflow-hidden {isDenied
      ? 'border-destructive/30 bg-destructive/5'
      : 'border-border/40 bg-surface-1/30'}"
    data-testid="trace-event"
    data-kind="tool_call_started"
    data-paired-as={isDenied ? "denied" : isCompleted ? "completed" : "pending"}
  >
    <button
      type="button"
      onclick={toggle}
      class="w-full flex items-start gap-2 px-2.5 py-2 text-left hover:bg-surface-1/50 transition-colors"
    >
      <div class="shrink-0 mt-0.5">
        {#if !isCompleted}
          <Spinner size={13} class="text-primary" />
        {:else if isDenied}
          <ShieldOff size={13} class="text-destructive" />
        {:else if success}
          <CheckCircle2 size={13} class="text-success" />
        {:else}
          <XCircle size={13} class="text-destructive" />
        {/if}
      </div>
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-1.5 text-body-xs text-foreground">
          {#if skin === "operator"}
            <span class="truncate">{operator}</span>
            {#if isDenied}
              <span class="font-mono text-caption text-destructive/80 shrink-0">
                · {$t("trace.tool_denied")}
              </span>
            {/if}
          {:else}
            <Wrench size={11} class="shrink-0 text-muted-foreground" />
            <span class="font-mono truncate">{toolName}</span>
            {#if isDenied}
              <span class="font-mono text-caption text-destructive/80 shrink-0">
                · denied ({String(deniedPayload?.reason ?? "")})
              </span>
            {:else if durationMs !== null && skin === "builder"}
              <span class="font-mono text-caption text-muted-foreground shrink-0">
                · {fmtDuration(durationMs)}
              </span>
            {/if}
          {/if}
        </div>
        {#if skin === "builder" && !expanded && argsJson}
          <div class="font-mono mt-0.5 truncate text-caption text-muted-foreground">
            {truncChars(argsJson, 100)}
          </div>
        {/if}
        {#if isDenied && deniedPayload?.detail}
          <div class="font-mono mt-0.5 text-caption text-muted-foreground">
            {String(deniedPayload.detail)}
          </div>
        {/if}
      </div>
    </button>
    {#if expanded && skin === "builder"}
      <div class="border-t border-border/40 px-2.5 py-2 space-y-2">
        {#if argsJson}
          <div>
            <div class="font-mono text-overline uppercase text-muted-foreground mb-1">
              {$t("trace.args_label")}
            </div>
            <pre class="font-mono text-caption text-foreground whitespace-pre-wrap break-all bg-surface-2/40 rounded p-1.5 max-h-60 overflow-y-auto">{JSON.stringify(safeParse(argsJson), null, 2)}</pre>
          </div>
        {/if}
        {#if completionPayload?.output_json}
          <div>
            <div class="font-mono text-overline uppercase text-muted-foreground mb-1">
              {$t("trace.output_label")}
            </div>
            <pre class="font-mono text-caption text-foreground whitespace-pre-wrap break-all bg-surface-2/40 rounded p-1.5 max-h-60 overflow-y-auto">{JSON.stringify(safeParse(String(completionPayload.output_json)), null, 2)}</pre>
          </div>
        {/if}
      </div>
    {/if}
  </div>

{:else if event.kind === "tool_call_completed"}
  {#if completion === null}
    <!-- Rendu seul si pas paired avec son started (cas exceptionnel -
         normalement ExecutionTrace fait le pairing en amont). -->
    {@const toolName = String(event.payload.tool_name ?? "")}
    {@const success = Boolean(event.payload.success)}
    <div class="flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-body-xs" data-testid="trace-event" data-kind="tool_call_completed">
      {#if success}
        <CheckCircle2 size={12} class="text-success" />
      {:else}
        <XCircle size={12} class="text-destructive" />
      {/if}
      <span class="font-mono text-muted-foreground">{toolName}</span>
    </div>
  {/if}

{:else if event.kind === "tool_call_denied"}
  {@const toolName = String(event.payload.tool_name ?? "")}
  {@const reason = String(event.payload.reason ?? "")}
  <div
    class="flex items-start gap-2 rounded-lg px-2.5 py-2 bg-destructive/5 border border-destructive/20"
    data-testid="trace-event"
    data-kind="tool_call_denied"
  >
    <ShieldOff size={13} class="shrink-0 mt-0.5 text-destructive" />
    <div class="min-w-0 flex-1">
      <div class="text-body-xs text-foreground">
        {#if skin === "operator"}
          The agent tried to use {toolName} but it was blocked.
        {:else}
          <span class="font-mono">{toolName}</span> denied - {reason}
          {#if event.payload.detail}
            <span class="text-muted-foreground"> ({String(event.payload.detail)})</span>
          {/if}
        {/if}
      </div>
    </div>
  </div>

{:else if event.kind === "a2a_invoke_started"}
  {@const skillId = String(event.payload.skill_id ?? "")}
  {#if skin === "operator" || expanded || skin === "builder"}
    <div
      class="flex items-start gap-2 rounded-lg px-2.5 py-2 bg-primary/5 border border-primary/20"
      data-testid="trace-event"
      data-kind="a2a_invoke_started"
    >
      <Network size={13} class="shrink-0 mt-0.5 text-primary" />
      <div class="min-w-0 flex-1">
        <div class="text-body-xs text-foreground">
          {$t("trace.delegated_to", { values: { agent: skillId } })}
        </div>
        {#if skin === "builder" && event.correlationId}
          <div class="font-mono mt-0.5 text-caption text-muted-foreground">
            chain: {truncChars(event.correlationId, 16)}
          </div>
        {/if}
      </div>
    </div>
  {/if}

{:else if event.kind === "llm_call_started"}
  {#if skin === "builder"}
    {@const backend = String(event.payload.backend ?? "")}
    {@const model = String(event.payload.model ?? "")}
    {@const messagesCount = Number(event.payload.messages_count ?? 0)}
    {@const promptChars = Number(event.payload.prompt_chars ?? 0)}
    <div
      class="flex items-center gap-2 rounded-lg px-2.5 py-1.5 bg-surface-1/30 text-caption"
      data-testid="trace-event"
      data-kind="llm_call_started"
    >
      <Cpu size={12} class="shrink-0 text-muted-foreground" />
      <span class="font-mono text-muted-foreground">
        {backend} / {model} · {messagesCount} msg · {promptChars} chars
      </span>
    </div>
  {/if}

{:else if event.kind === "llm_call_failed"}
  {@const errorKind = String(
    (event.payload.analysis as Record<string, unknown> | undefined)?.category ?? "error",
  )}
  <div
    class="flex items-start gap-2 rounded-lg px-2.5 py-2 bg-destructive/5 border border-destructive/20"
    data-testid="trace-event"
    data-kind="llm_call_failed"
  >
    <XCircle size={13} class="shrink-0 mt-0.5 text-destructive" />
    <div class="min-w-0 flex-1">
      <div class="text-body-xs text-foreground">
        {$t("trace.llm_failed")}
        {#if skin === "builder"}
          <span class="font-mono text-muted-foreground"> · {errorKind}</span>
        {/if}
      </div>
      {#if skin === "builder" && event.payload.error}
        <div class="font-mono mt-0.5 text-caption text-muted-foreground whitespace-pre-wrap">
          {String(event.payload.error)}
        </div>
      {/if}
    </div>
  </div>

{:else if event.kind === "retry"}
  {@const cause = String(event.payload.cause ?? "")}
  {@const attempt = Number(event.payload.attempt ?? 1)}
  <div class="flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-caption text-muted-foreground" data-testid="trace-event" data-kind="retry">
    <RefreshCw size={12} class="shrink-0" />
    <span>
      {$t("trace.retry_attempt", { values: { n: attempt } })}
      {#if skin === "builder"}
        <span class="font-mono"> · {cause}</span>
      {/if}
    </span>
  </div>

{:else if event.kind === "action_parse_error"}
  {#if skin === "builder"}
    {@const raw = String(event.payload.raw_content ?? "")}
    <div
      class="rounded-lg border border-warning/30 bg-warning/5 overflow-hidden"
      data-testid="trace-event"
      data-kind="action_parse_error"
    >
      <button
        type="button"
        onclick={toggle}
        class="w-full flex items-start gap-2 px-2.5 py-2 text-left hover:bg-warning/10 transition-colors"
      >
        <AlertTriangle size={13} class="shrink-0 mt-0.5 text-warning" />
        <div class="min-w-0 flex-1 text-body-xs text-foreground">
          {$t("trace.parse_error")}
        </div>
      </button>
      {#if expanded}
        <div class="border-t border-warning/20 px-2.5 py-2">
          <pre class="font-mono text-caption text-foreground whitespace-pre-wrap break-all bg-surface-2/40 rounded p-1.5 max-h-60 overflow-y-auto">{raw}</pre>
        </div>
      {/if}
    </div>
  {:else}
    <div class="flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-caption text-muted-foreground" data-testid="trace-event" data-kind="action_parse_error">
      <AlertTriangle size={12} class="shrink-0 text-warning" />
      <span>{$t("trace.parse_error")}</span>
    </div>
  {/if}

{:else}
  <!-- Forward-compat : kinds inconnus (variants futurs) - affichage minimal en builder -->
  {#if skin === "builder"}
    <div class="flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-caption text-muted-foreground" data-testid="trace-event" data-kind={event.kind}>
      <MessageSquare size={11} class="shrink-0" />
      <span class="font-mono">{event.kind}</span>
    </div>
  {/if}
{/if}
