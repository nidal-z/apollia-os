<script lang="ts">
  /**
   * Unified reasoning item renderer.
   *
   * Replaces `ToolCallCard`, `BuilderToolCard`, `OperatorToolCard`,
   * `WebSearchResultsCard`, `WebReadCard`, `ReasoningTraceCard`, and
   * `ThinkingBadge`. Approvals stay in `ApprovalCard(V2)` but share the
   * `ReasoningCardShell` primitive.
   *
   * Layout is driven by `item.kind`; `skin` selects between builder (raw
   * details) and operator (semantic description) presentation.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { JSON_LINE_THRESHOLD } from "$lib/chat/reasoning";
  import ReasoningCardShell from "./ReasoningCardShell.svelte";
  import { t } from "svelte-i18n";
  import {
    Check,
    X,
    ExternalLink,
    Compass,
    BookOpen,
    RotateCcw,
    Quote,
    AlertTriangle,
    Wrench,
  } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import {
    resolveToolDisplay,
    buildBashInputDisplay,
    buildHttpInputDisplay,
    buildOutputSummary,
    formatRationale,
  } from "$lib/tools/tool-display";
  import type { ToolCallView } from "$lib/types";
  import PerformanceHint from "./PerformanceHint.svelte";
  import RetryTimeline from "./RetryTimeline.svelte";
  import { Separator } from "$lib/components/ui/separator";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    item: ReasoningItem;
    /** `builder` shows raw args/output; `operator` shows a semantic description. */
    skin?: "builder" | "operator";
    /** Optional override for the default expansion state. */
    defaultExpanded?: boolean;
    /** When false, expand state is not written to sessionStorage. */
    persist?: boolean;
    /** Opens the citation side panel - consumer wires the actual slide-over. */
    onCitation?: (item: Extract<ReasoningItem, { kind: "citation" }>) => void;
  }

  let {
    item,
    skin = "builder",
    defaultExpanded,
    persist = true,
    onCitation,
  }: Props = $props();

  const storageKey = $derived(`apollia.reasoning.expanded.${item.id}`);

  function loadExpanded(): boolean {
    if (!persist || typeof sessionStorage === "undefined") {
      return defaultExpanded ?? defaultForKind(item.kind);
    }
    const raw = sessionStorage.getItem(storageKey);
    if (raw === "1") return true;
    if (raw === "0") return false;
    return defaultExpanded ?? defaultForKind(item.kind);
  }

  function defaultForKind(kind: ReasoningItem["kind"]): boolean {
    // All variants collapsed by default - the user can expand on demand.
    // Retry chains stay open since they are short and signal a problem
    // worth surfacing immediately.
    return kind === "retry";
  }

  let expanded = $state(loadExpanded());

  function toggle() {
    expanded = !expanded;
    if (persist && typeof sessionStorage !== "undefined") {
      try {
        sessionStorage.setItem(storageKey, expanded ? "1" : "0");
      } catch {
        // quota or privacy mode - ignore
      }
    }
  }

  function hostname(url: string): string {
    try {
      return new URL(url).hostname.replace(/^www\./, "");
    } catch {
      return url;
    }
  }

  // ---- JSON preview with 600-line threshold ----
  let showFullJson = $state(false);

  function formatJson(value: unknown): string {
    try {
      return JSON.stringify(value, null, 2);
    } catch {
      return String(value);
    }
  }

  function truncateJson(raw: string, max = JSON_LINE_THRESHOLD): {
    preview: string;
    truncated: boolean;
  } {
    const lines = raw.split("\n");
    if (lines.length <= max) return { preview: raw, truncated: false };
    return {
      preview: lines.slice(0, max).join("\n") + "\n…",
      truncated: true,
    };
  }

  // ---- Tool-call specific derivations ----
  const toolDisplay = $derived.by(() => {
    if (item.kind !== "tool_call") return null;
    const view: ToolCallView = {
      tool_name: item.tool,
      input: item.args,
      output: item.output,
      status:
        item.status === "success"
          ? "executed"
          : item.status === "rejected" || item.status === "error"
            ? "refused"
            : item.status === "running" || item.status === "approved"
              ? "authorized"
              : "pending",
      duration_ms: item.duration_ms,
      exit_code: item.exit_code,
    };
    return resolveToolDisplay(view);
  });

  const bashDisplay = $derived(
    item.kind === "tool_call" && item.tool === "bash_executor"
      ? buildBashInputDisplay(item.args)
      : null,
  );
  const httpDisplay = $derived(
    item.kind === "tool_call" && item.tool === "http_fetch"
      ? buildHttpInputDisplay(item.args)
      : null,
  );

  const argsJson = $derived(
    item.kind === "tool_call" ? formatJson(item.args) : "",
  );
  const argsPreview = $derived(truncateJson(argsJson));

  // Structured rationale. Opt-in - `null` when the user has
  // disabled "Explain tool calls" or the meta-LLM fallback kicked in.
  const rationale = $derived(
    item.kind === "tool_call" ? formatRationale(item.rationale) : null,
  );

  // Structured retry chain. Non-empty only when the resilience
  // layer recorded at least one attempt for this invocation.
  const retryAttempts = $derived(
    item.kind === "tool_call" ? (item.retry_attempts ?? []) : [],
  );
  const retryCount = $derived(Math.max(0, retryAttempts.length - 1));

  // ---- ask_user parsing (shared by both skins) ----
  // The `ask_user` tool stores questions in `input.questions` and answers in
  // `output` as `{answers: [{id, value?, values?, skipped}]}`. Raw JSON is
  // unreadable to operators, so we parse both sides into a paired Q/A list.
  interface AskUserAnswer {
    id: string;
    value?: string | null;
    values?: string[];
    skipped: boolean;
  }
  interface AskUserQuestion {
    id: string;
    question: string;
    options?: string[];
  }
  interface AskUserPair {
    id: string;
    question: string;
    answerText: string;
    skipped: boolean;
  }

  const askUserPairs = $derived.by<AskUserPair[] | null>(() => {
    if (item.kind !== "tool_call" || item.tool !== "ask_user") return null;
    const rawQuestions = (item.args as { questions?: unknown }).questions;
    const questions = Array.isArray(rawQuestions)
      ? (rawQuestions as AskUserQuestion[])
      : [];
    let answers: AskUserAnswer[] = [];
    if (typeof item.output === "string" && item.output.length > 0) {
      try {
        const parsed = JSON.parse(item.output) as { answers?: AskUserAnswer[] };
        if (Array.isArray(parsed?.answers)) answers = parsed.answers;
      } catch {
        return null;
      }
    }
    if (questions.length === 0 && answers.length === 0) return null;
    const byId = new Map(questions.map((q) => [q.id, q]));
    const pairs: AskUserPair[] = [];
    if (answers.length > 0) {
      for (const a of answers) {
        const q = byId.get(a.id);
        const text = a.skipped
          ? $t("chat.ask_user_skipped_label", { default: "Skipped" })
          : a.values && a.values.length > 0
            ? a.values.join(", ")
            : (a.value ?? "");
        pairs.push({
          id: a.id,
          question: q?.question ?? a.id,
          answerText: text,
          skipped: a.skipped,
        });
      }
    } else {
      for (const q of questions) {
        pairs.push({
          id: q.id,
          question: q.question,
          answerText: "",
          skipped: false,
        });
      }
    }
    return pairs;
  });

  const askUserCounts = $derived.by(() => {
    if (!askUserPairs) return null;
    const answered = askUserPairs.filter((p) => !p.skipped && p.answerText).length;
    const skipped = askUserPairs.filter((p) => p.skipped).length;
    return { answered, skipped, total: askUserPairs.length };
  });

  // ---- Operator-skin output summary ----
  // Prefer the tool's i18n outputSummaryKey (rich, localized), fall back to
  // the technical buildOutputSummary, and finally to a one-line peek at raw
  // output. Returns null only when nothing meaningful can be shown (e.g.
  // pending/running state with no output yet).
  const operatorOutputSummary = $derived.by(() => {
    if (item.kind !== "tool_call") return null;
    if (item.status !== "success" && item.status !== "approved") return null;
    if (!toolDisplay) return null;
    // ask_user renders its own Q/A block - skip the generic summary.
    if (item.tool === "ask_user") return null;
    if (toolDisplay.outputSummaryKey) {
      return $t(toolDisplay.outputSummaryKey, {
        values: toolDisplay.outputParams,
      });
    }
    const technical = buildOutputSummary(item.tool, toolDisplay.outputParams);
    if (technical) return technical;
    if (typeof item.output === "string" && item.output.length > 0) {
      const firstLine = item.output.split("\n")[0] ?? "";
      return firstLine.slice(0, 200);
    }
    return null;
  });

  // ---- Operator-skin "what was called" line ----
  // Shown in the body when the title is occupied by a narrative rationale
  // summary, so the operator still sees the concrete target (path, URL,
  // command…). Suppressed when the title already carries this info.
  const operatorTargetLine = $derived.by(() => {
    if (item.kind !== "tool_call") return null;
    if (skin !== "operator") return null;
    if (!toolDisplay) return null;
    if (!rationale?.summary) return null; // title already shows description
    return $t(toolDisplay.descriptionKey, {
      values: toolDisplay.templateParams,
    });
  });

  // ---- web_read content preview ----
  const READ_PREVIEW_CHARS = 500;
  let showFullRead = $state(false);
  const readPreview = $derived.by(() => {
    if (item.kind !== "web_read") return "";
    if (item.extracted.length <= READ_PREVIEW_CHARS) return item.extracted;
    return item.extracted.slice(0, READ_PREVIEW_CHARS) + "…";
  });

  const testid = $derived(`reasoning-card-${item.kind}`);
</script>

{#snippet statusBadge(duration: number | null | undefined)}
  {#if item.status === "pending" || item.status === "running"}
    <Spinner class="h-3 w-3 text-muted-foreground" />
  {:else if item.status === "success" || item.status === "approved"}
    <Check class="h-3 w-3 text-success" />
    {#if duration != null}
      <span class="text-[10px] text-muted-foreground">{duration}ms</span>
    {/if}
  {:else if item.status === "error" || item.status === "rejected"}
    <X class="h-3 w-3 text-destructive" />
  {/if}
{/snippet}

{#if item.kind === "tool_call"}
  {@const isError = item.status === "error" || item.status === "rejected"}
  {@const isRunning = item.status === "running" || item.status === "pending"}
  <div class="my-1.5" data-testid={testid}>
    <button
      type="button"
      class="group flex w-full items-center gap-1.5 text-[11px] text-muted-foreground hover:text-foreground transition-colors"
      aria-expanded={expanded}
      aria-label={$t("chat.reasoning.toggle_tool", {
        default: "Toggle tool call details",
      })}
      onclick={toggle}
    >
      <span
        class="inline-block leading-none transition-transform duration-150"
        class:rotate-90={expanded}
      >›</span>
      {#if toolDisplay}
        {@const ToolIcon = toolDisplay.icon}
        <ToolIcon
          class="h-3 w-3 flex-shrink-0 {isError
            ? 'text-destructive/80'
            : 'opacity-70'}"
        />
      {:else}
        <Wrench
          class="h-3 w-3 flex-shrink-0 {isError
            ? 'text-destructive/80'
            : 'opacity-70'}"
        />
      {/if}
      <span
        class="min-w-0 truncate font-medium {isError
          ? 'text-destructive'
          : 'text-foreground/85'}"
      >
        {#if skin === "operator" && toolDisplay}
          {rationale?.summary ??
            $t(toolDisplay.descriptionKey, {
              values: toolDisplay.templateParams,
            })}
        {:else}
          <span class="font-mono">{item.tool}</span>
        {/if}
      </span>
      {#if skin === "operator" && rationale?.summary == null && toolDisplay && (bashDisplay || httpDisplay)}
        <span
          class="ml-1 hidden min-w-0 truncate font-mono text-[10px] text-muted-foreground/70 sm:inline"
        >{bashDisplay ?? httpDisplay}</span>
      {/if}
      <span class="ml-auto flex flex-shrink-0 items-center gap-1.5">
        {@render statusBadge(item.duration_ms)}
        {#if isError && item.exit_code != null}
          <span class="text-[10px] text-destructive">exit {item.exit_code}</span>
        {/if}
        {#if rationale?.performance_hint}
          <PerformanceHint hint={rationale.performance_hint} />
        {/if}
        {#if retryCount > 0}
          <RetryTimeline attempts={retryAttempts} skin="operator" />
        {/if}
      </span>
    </button>

    {#if expanded}
      <div class="mt-1 space-y-1.5 pl-4 text-[11px] leading-relaxed">
        {#if askUserPairs}
          <div class="space-y-1">
            <div class="flex items-center gap-1.5 text-[10px] uppercase tracking-wide text-muted-foreground/60">
              <span>{$t("chat.ask_user_qa_label", { default: "Questions & answers" })}</span>
              {#if askUserCounts}
                <span class="text-muted-foreground/50 normal-case tracking-normal">
                  · {askUserCounts.answered}/{askUserCounts.total}
                  {#if askUserCounts.skipped > 0}
                    · {askUserCounts.skipped} {$t("chat.ask_user_skipped_label", { default: "skipped" })}
                  {/if}
                </span>
              {/if}
              <Separator variant="inline" />
            </div>
            <ul class="space-y-1.5">
              {#each askUserPairs as pair (pair.id)}
                <li class="space-y-0.5">
                  <p class="text-foreground/85">{pair.question}</p>
                  <p class="pl-2 border-l-2 {pair.skipped ? 'border-muted-foreground/30 italic text-muted-foreground' : 'border-success/40 text-foreground'}">
                    {pair.skipped
                      ? $t("chat.ask_user_skipped_label", { default: "Skipped" })
                      : (pair.answerText || $t("chat.ask_user_pending", { default: "Pending…" }))}
                  </p>
                </li>
              {/each}
            </ul>
          </div>
        {/if}
        {#if rationale}
          <div data-testid="tool-rationale-header" class="space-y-0.5">
            <p class="text-foreground/85">
              <span class="text-muted-foreground/70">{$t("chat.reasoning.rationale_label", { default: "Rationale" })}:</span>
              {rationale.summary}
            </p>
            {#if rationale.inputs_recap.length > 0}
              <ul class="flex flex-wrap gap-x-3 gap-y-0.5 text-[10px] text-muted-foreground">
                {#each rationale.inputs_recap as [k, v] (k)}
                  <li class="font-mono">
                    <span class="opacity-60">{k}</span>
                    <span class="mx-0.5 opacity-40">=</span>
                    <span class="text-foreground/75">{v}</span>
                  </li>
                {/each}
              </ul>
            {/if}
            <p class="text-[10px] italic text-muted-foreground/80">
              → {rationale.expected_outcome}
            </p>
          </div>
        {/if}

        {#if skin === "builder" && !askUserPairs}
          <div class="space-y-1">
            <div class="flex items-center gap-1.5 text-[10px] uppercase tracking-wide text-muted-foreground/60">
              <span>{$t("chat.reasoning.input_label", { default: "Input" })}</span>
              <Separator variant="inline" />
            </div>
            {#if bashDisplay !== null}
              <pre
                class="rounded bg-muted/30 px-2 py-1 font-mono text-foreground overflow-x-auto"
              ><code>{bashDisplay}</code></pre>
            {:else if httpDisplay !== null}
              <p class="font-mono text-foreground/85 break-all">{httpDisplay}</p>
            {:else}
              <pre
                class="rounded bg-muted/30 px-2 py-1 font-mono text-foreground overflow-x-auto whitespace-pre-wrap break-all"
              ><code>{showFullJson ? argsJson : argsPreview.preview}</code></pre>
              {#if argsPreview.truncated}
                <button
                  type="button"
                  class="text-[10px] text-muted-foreground hover:text-foreground transition-colors"
                  onclick={(e) => {
                    e.stopPropagation();
                    showFullJson = !showFullJson;
                  }}
                >
                  {showFullJson
                    ? $t("chat.tool_collapse")
                    : $t("chat.reasoning.see_all", { default: "See all" })}
                </button>
              {/if}
            {/if}
          </div>

          {#if item.output !== null && (item.status === "success" || item.status === "error")}
            <div class="space-y-1">
              <div class="flex items-center gap-1.5 text-[10px] uppercase tracking-wide {isError ? 'text-destructive/70' : 'text-muted-foreground/60'}">
                <span>{$t("chat.reasoning.output_label", { default: "Output" })}</span>
                <Separator variant="inline" class={isError ? "bg-destructive/30" : undefined} />
              </div>
              <pre
                class="rounded {isError
                  ? 'bg-destructive/5 text-destructive/90'
                  : 'bg-muted/30 text-foreground'} px-2 py-1 font-mono overflow-x-auto whitespace-pre-wrap break-all"
              ><code>{item.output}</code></pre>
            </div>
          {/if}
        {:else}
          {#if operatorTargetLine}
            <p class="text-foreground/80">
              <span class="text-muted-foreground/70">{$t("chat.reasoning.target_label", { default: "Target" })}:</span>
              <span class="font-mono text-[10.5px]">{operatorTargetLine}</span>
            </p>
          {/if}
          {#if bashDisplay !== null}
            <div class="space-y-1">
              <div class="flex items-center gap-1.5 text-[10px] uppercase tracking-wide text-muted-foreground/60">
                <span>{$t("chat.reasoning.command_label", { default: "Command" })}</span>
                <Separator variant="inline" />
              </div>
              <pre
                class="rounded bg-muted/30 px-2 py-1 font-mono text-foreground overflow-x-auto"
              ><code>{bashDisplay}</code></pre>
            </div>
          {:else if httpDisplay !== null}
            <p class="font-mono text-foreground/85 break-all text-[10.5px]">{httpDisplay}</p>
          {/if}
          {#if operatorOutputSummary}
            <p class="text-foreground/85">
              <span class="text-muted-foreground/70">{$t("chat.reasoning.result_label", { default: "Result" })}:</span>
              {operatorOutputSummary}
            </p>
          {/if}
          {#if item.output !== null && isError}
            <div class="space-y-1">
              <div class="flex items-center gap-1.5 text-[10px] uppercase tracking-wide text-destructive/70">
                <span>{$t("chat.reasoning.error_label", { default: "Error" })}</span>
                <span class="h-px flex-1 bg-destructive/30"></span>
              </div>
              <p class="text-destructive">
                {item.output.split("\n")[0]?.slice(0, 240) ?? ""}
              </p>
            </div>
          {/if}
          {#if isRunning && !rationale && !operatorTargetLine && !bashDisplay && !httpDisplay}
            <p class="italic text-muted-foreground/70">
              {$t("chat.reasoning.running_hint", {
                default: "Running…",
              })}
            </p>
          {/if}
        {/if}

        {#if retryAttempts.length > 0 && (skin === "builder" || retryCount > 0)}
          <div class="pt-0.5">
            <RetryTimeline attempts={retryAttempts} skin={skin} />
          </div>
        {/if}
      </div>
    {/if}
  </div>
{:else if item.kind === "web_search"}
  <ReasoningCardShell
    status={item.status}
    testid={testid}
    collapsible
    expanded={expanded}
    onToggle={toggle}
    ariaLabel="web_search"
  >
    {#snippet icon()}
      <Compass class="h-3 w-3 text-muted-foreground" />
    {/snippet}
    {#snippet title()}web_search{/snippet}
    {#snippet meta()}{@render statusBadge(item.duration_ms)}{/snippet}
    {#snippet body()}
      {#if item.query}
        <p class="text-[11px] text-muted-foreground">
          <span class="font-mono">“{item.query}”</span>
          {#if item.backend}
            <span class="mx-1">·</span><span>{item.backend}</span>
          {/if}
          {#if item.total_results != null}
            <span class="mx-1">·</span>
            <span>{$t("tools.output.web_search_summary", {
              values: {
                total_results: item.total_results,
                backend: item.backend ?? "",
                duration_ms: item.duration_ms ?? 0,
              },
            })}</span>
          {/if}
        </p>
      {/if}
      {#if item.results.length > 0}
        <ol class="mt-2 space-y-2">
          {#each item.results as r (r.rank)}
            <li class="rounded-md glass-inset p-2">
              <a
                href={r.url}
                target="_blank"
                rel="noopener noreferrer"
                class="group block"
              >
                <div class="flex items-start justify-between gap-2">
                  <span
                    class="text-[12px] font-medium text-foreground group-hover:underline line-clamp-2"
                  >{r.title}</span>
                  <ExternalLink
                    class="mt-0.5 h-3 w-3 flex-shrink-0 text-muted-foreground opacity-0 group-hover:opacity-100 transition-opacity"
                  />
                </div>
                <div class="mt-0.5 flex items-center gap-1 text-[10px] text-muted-foreground">
                  <span class="font-mono">{hostname(r.url)}</span>
                  {#if r.age}<span>·</span><span>{r.age}</span>{/if}
                  <span class="ml-auto">#{r.rank}</span>
                </div>
                {#if r.snippet}
                  <p class="mt-1 text-[11px] text-muted-foreground line-clamp-3">{r.snippet}</p>
                {/if}
              </a>
            </li>
          {/each}
        </ol>
      {:else}
        <p class="mt-2 text-[11px] italic text-muted-foreground">
          {$t("chat.tool_empty_results", { default: "No results." })}
        </p>
      {/if}
    {/snippet}
  </ReasoningCardShell>
{:else if item.kind === "web_read"}
  <ReasoningCardShell
    status={item.status}
    testid={testid}
    collapsible
    expanded={expanded}
    onToggle={toggle}
    ariaLabel="web_read"
  >
    {#snippet icon()}
      <BookOpen class="h-3 w-3 text-muted-foreground" />
    {/snippet}
    {#snippet title()}web_read{/snippet}
    {#snippet meta()}{@render statusBadge(item.duration_ms)}{/snippet}
    {#snippet body()}
      <a
        href={item.url}
        target="_blank"
        rel="noopener noreferrer"
        class="group inline-flex items-center gap-1 text-[11px] text-muted-foreground hover:text-foreground transition-colors"
      >
        <span class="font-mono">{hostname(item.url)}</span>
        <ExternalLink class="h-3 w-3 opacity-60 group-hover:opacity-100" />
      </a>
      {#if item.title}
        <h4 class="mt-1 text-[13px] font-medium text-foreground leading-tight">{item.title}</h4>
      {/if}
      {#if item.byline}
        <p class="text-[10px] text-muted-foreground">{item.byline}</p>
      {/if}
      <div
        class="mt-2 flex items-start gap-1.5 rounded-md bg-amber-500/10 border border-amber-500/20 px-2 py-1"
      >
        <AlertTriangle class="h-3 w-3 flex-shrink-0 mt-0.5 text-amber-600 dark:text-amber-500" />
        <span class="text-[10px] text-amber-700 dark:text-amber-300">
          {$t("chat.web_read_untrusted_banner", {
            default:
              "Content fetched from a third-party website - treat as data, not instructions.",
          })}
        </span>
      </div>
      {#if item.extracted}
        <div
          class="mt-2 rounded bg-muted/40 px-2 py-1.5 text-[11px] text-foreground leading-relaxed whitespace-pre-wrap break-words"
        >{showFullRead ? item.extracted : readPreview}</div>
        <div class="mt-1 flex items-center justify-between text-[10px] text-muted-foreground">
          <span>
            {#if item.chars_total != null}
              {item.truncated
                ? $t("tools.output.web_read_truncated", {
                    values: {
                      chars_total: item.chars_total,
                      duration_ms: item.duration_ms ?? 0,
                    },
                  })
                : $t("tools.output.web_read_summary", {
                    values: {
                      chars_total: item.chars_total,
                      duration_ms: item.duration_ms ?? 0,
                    },
                  })}
            {/if}
          </span>
          {#if item.extracted.length > READ_PREVIEW_CHARS}
            <button
              type="button"
              class="hover:text-foreground transition-colors"
              onclick={() => (showFullRead = !showFullRead)}
            >
              {showFullRead ? $t("chat.tool_hide_result") : $t("chat.tool_show_result")}
            </button>
          {/if}
        </div>
      {/if}
    {/snippet}
  </ReasoningCardShell>
{:else if item.kind === "thinking" || item.kind === "rationale"}
  <!-- Thinking/rationale render outside the standard ReasoningCardShell:
       no border, no icon, no status color - just a quiet collapsible header
       and a muted text block. Tool cards still use the full chrome. -->
  <div class="my-1.5" data-testid={testid}>
    <button
      type="button"
      class="inline-flex items-center gap-1 text-[11px] text-muted-foreground hover:text-foreground transition-colors"
      aria-expanded={expanded}
      onclick={toggle}
    >
      <span class="inline-block leading-none transition-transform duration-150" class:rotate-90={expanded}>›</span>
      <span class="font-medium">
        {item.kind === "thinking"
          ? $t("chat.reasoning.thinking_label", { default: "Thinking" })
          : $t("chat.reasoning.rationale_label", { default: "Rationale" })}
      </span>
    </button>
    {#if expanded}
      <div
        class="mt-1 pl-3 text-[12px] leading-relaxed text-muted-foreground/85 whitespace-pre-wrap"
      >{item.content}</div>
    {/if}
  </div>
{:else if item.kind === "retry"}
  <ReasoningCardShell
    status={item.status}
    testid={testid}
    collapsible
    expanded={expanded}
    onToggle={toggle}
  >
    {#snippet icon()}<RotateCcw class="h-3 w-3 text-muted-foreground" />{/snippet}
    {#snippet title()}
      {$t("chat.reasoning.retry_label", {
        default: "Retry ({n} attempts)",
        values: { n: item.attempts.length },
      })}
    {/snippet}
    {#snippet body()}
      <ol class="space-y-1">
        {#each item.attempts as a (a.index)}
          <li
            class="flex items-center gap-2 rounded px-2 py-1 text-[11px] {a.status === 'success'
              ? 'bg-success/5 text-success'
              : a.status === 'error' || a.status === 'rejected'
                ? 'bg-destructive/5 text-destructive'
                : 'bg-muted/30 text-muted-foreground'}"
          >
            <span class="font-mono text-[10px] opacity-70">#{a.index}</span>
            <span class="flex-1 truncate">{a.error ?? a.status}</span>
            {#if a.duration_ms != null}
              <span class="text-[10px] opacity-70">{a.duration_ms}ms</span>
            {/if}
          </li>
        {/each}
      </ol>
      {#if item.final_error}
        <p class="mt-1 text-[11px] text-destructive">{item.final_error}</p>
      {/if}
    {/snippet}
  </ReasoningCardShell>
{:else if item.kind === "citation"}
  <ReasoningCardShell status={item.status} testid={testid}>
    {#snippet icon()}<Quote class="h-3 w-3 text-muted-foreground" />{/snippet}
    {#snippet title()}
      <Button variant="ghost" size="sm"
        type="button"
        class="truncate text-left hover:underline"
        onclick={() => onCitation?.(item)}
      >{item.source}</Button>
    {/snippet}
    {#snippet body()}
      <p class="text-[11px] text-muted-foreground line-clamp-3">{item.excerpt}</p>
      {#if item.url}
        <a
          href={item.url}
          target="_blank"
          rel="noopener noreferrer"
          class="mt-1 inline-flex items-center gap-1 text-[10px] text-muted-foreground hover:text-foreground"
        >
          <span class="font-mono">{hostname(item.url)}</span>
          <ExternalLink class="h-3 w-3" />
        </a>
      {/if}
    {/snippet}
  </ReasoningCardShell>
{/if}
