<script lang="ts">
  /**
   * Unified reasoning item renderer (US-SP42-028).
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
    BrainCircuit,
    Lightbulb,
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
    formatRationale,
  } from "$lib/tools/tool-display";
  import type { ToolCallView } from "$lib/types";
  import PerformanceHint from "./PerformanceHint.svelte";

  interface Props {
    item: ReasoningItem;
    /** `builder` shows raw args/output; `operator` shows a semantic description. */
    skin?: "builder" | "operator";
    /** Optional override for the default expansion state. */
    defaultExpanded?: boolean;
    /** When false, expand state is not written to sessionStorage. */
    persist?: boolean;
    /** Opens the citation side panel — consumer wires the actual slide-over. */
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
    // Thinking / rationale / retry expanded by default; tool calls collapsed.
    return kind === "thinking" || kind === "rationale" || kind === "retry";
  }

  let expanded = $state(loadExpanded());

  function toggle() {
    expanded = !expanded;
    if (persist && typeof sessionStorage !== "undefined") {
      try {
        sessionStorage.setItem(storageKey, expanded ? "1" : "0");
      } catch {
        // quota or privacy mode — ignore
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

  // ---- JSON preview with 600-line threshold (B.32) ----
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

  // Structured rationale (US-SP42-038). Opt-in — `null` when the user has
  // disabled "Explain tool calls" or the meta-LLM fallback kicked in.
  const rationale = $derived(
    item.kind === "tool_call" ? formatRationale(item.rationale) : null,
  );

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
  <ReasoningCardShell
    status={item.status}
    testid={testid}
    collapsible
    expanded={expanded}
    onToggle={toggle}
    ariaLabel={$t("chat.reasoning.toggle_tool", {
      default: "Toggle tool call details",
    })}
  >
    {#snippet icon()}
      {#if toolDisplay}
        {@const ToolIcon = toolDisplay.icon}
        <ToolIcon class="h-3 w-3 text-muted-foreground" />
      {:else}
        <Wrench class="h-3 w-3 text-muted-foreground" />
      {/if}
    {/snippet}
    {#snippet title()}
      {#if skin === "operator" && toolDisplay}
        <span class="text-[13px] font-medium text-foreground">
          {rationale?.summary ??
            $t(toolDisplay.descriptionKey, {
              values: toolDisplay.templateParams,
            })}
        </span>
      {:else}
        {item.tool}
      {/if}
    {/snippet}
    {#snippet meta()}
      {@render statusBadge(item.duration_ms)}
      {#if (item.status === "error" || item.status === "rejected") && item.exit_code != null}
        <span class="text-[10px] text-destructive">exit {item.exit_code}</span>
      {/if}
      {#if rationale?.performance_hint}
        <PerformanceHint hint={rationale.performance_hint} />
      {/if}
    {/snippet}
    {#snippet body()}
      {#if skin === "builder" && rationale}
        <div
          class="mb-1.5 rounded border border-border/30 bg-muted/20 px-2 py-1.5 text-[11px] leading-relaxed"
          data-testid="tool-rationale-header"
        >
          <p class="font-medium text-foreground/90">{rationale.summary}</p>
          {#if rationale.inputs_recap.length > 0}
            <ul class="mt-1 flex flex-wrap gap-x-2 gap-y-0.5 text-[10px] text-muted-foreground">
              {#each rationale.inputs_recap as [k, v] (k)}
                <li class="font-mono">
                  <span class="opacity-60">{k}</span>
                  <span class="mx-0.5 opacity-40">=</span>
                  <span class="text-foreground/80">{v}</span>
                </li>
              {/each}
            </ul>
          {/if}
          <p class="mt-1 text-[10px] italic text-muted-foreground">
            → {rationale.expected_outcome}
          </p>
        </div>
      {/if}
      {#if skin === "builder"}
        {#if bashDisplay !== null}
          <pre
            class="rounded bg-muted/40 px-2 py-1 text-[11px] font-mono text-foreground overflow-x-auto"
          ><code>{bashDisplay}</code></pre>
        {:else if httpDisplay !== null}
          <p class="text-[11px] font-mono text-muted-foreground">{httpDisplay}</p>
        {:else}
          <pre
            class="rounded bg-muted/40 px-2 py-1 text-[11px] font-mono text-foreground overflow-x-auto whitespace-pre-wrap break-all"
          ><code>{showFullJson ? argsJson : argsPreview.preview}</code></pre>
          {#if argsPreview.truncated}
            <button
              type="button"
              class="mt-0.5 text-[10px] text-muted-foreground hover:text-foreground transition-colors"
              onclick={() => (showFullJson = !showFullJson)}
            >
              {showFullJson
                ? $t("chat.tool_collapse")
                : $t("chat.reasoning.see_all", { default: "See all" })}
            </button>
          {/if}
        {/if}
        {#if item.output !== null && (item.status === "success" || item.status === "error")}
          <pre
            class="mt-1.5 rounded bg-muted/40 px-2 py-1 text-[11px] font-mono text-foreground overflow-x-auto whitespace-pre-wrap break-all"
          ><code>{item.output}</code></pre>
        {/if}
      {:else if item.output !== null && item.status === "error"}
        <p class="text-[11px] text-destructive">
          {item.output.split("\n")[0]?.slice(0, 160) ?? ""}
        </p>
      {/if}
    {/snippet}
  </ReasoningCardShell>
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
              "Content fetched from a third-party website — treat as data, not instructions.",
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
  <ReasoningCardShell
    status={item.status}
    testid={testid}
    collapsible
    expanded={expanded}
    onToggle={toggle}
  >
    {#snippet icon()}
      {#if item.kind === "thinking"}
        <BrainCircuit class="h-3 w-3 text-primary/70" />
      {:else}
        <Lightbulb class="h-3 w-3 text-primary/70" />
      {/if}
    {/snippet}
    {#snippet title()}
      <span class="italic">
        {item.kind === "thinking"
          ? $t("chat.reasoning.thinking_label", { default: "Thinking" })
          : $t("chat.reasoning.rationale_label", { default: "Rationale" })}
      </span>
    {/snippet}
    {#snippet body()}
      <div
        class="rounded glass-inset px-2.5 py-2 text-[11px] italic leading-relaxed text-muted-foreground/80 whitespace-pre-wrap"
      >{item.content}</div>
    {/snippet}
  </ReasoningCardShell>
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
      <button
        type="button"
        class="truncate text-left hover:underline"
        onclick={() => onCitation?.(item)}
      >{item.source}</button>
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
