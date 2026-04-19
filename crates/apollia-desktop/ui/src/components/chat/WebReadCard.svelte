<script lang="ts">
  /**
   * Rich renderer for `web_read` tool output.
   *
   * Shows the extracted article with source URL, title, byline, and text —
   * with an "untrusted content" banner (ADR-072 risk acknowledgement) and
   * a collapsible full-text view.
   */

  import type { ToolCallView } from "$lib/types";
  import { Check, X, ExternalLink, BookOpen, AlertTriangle } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import { t } from "svelte-i18n";

  let { toolCall }: { toolCall: ToolCallView } = $props();

  interface WebReadOutput {
    url: string;
    title: string | null;
    byline: string | null;
    content: string;
    chars_total: number;
    truncated: boolean;
    duration_ms: number;
  }

  const parsed: WebReadOutput | null = $derived.by(() => {
    if (!toolCall.output) return null;
    try {
      const v = JSON.parse(toolCall.output) as WebReadOutput;
      if (!v || typeof v.content !== "string") return null;
      return v;
    } catch {
      return null;
    }
  });

  let showFull = $state(false);

  const isPending = $derived(
    toolCall.status === "pending" || toolCall.status === "authorized",
  );
  const isRefused = $derived(toolCall.status === "refused");
  const isExecuted = $derived(toolCall.status === "executed");

  const preview = $derived.by(() => {
    if (!parsed) return "";
    const MAX = 500;
    if (parsed.content.length <= MAX) return parsed.content;
    return parsed.content.slice(0, MAX) + "…";
  });

  function hostname(url: string): string {
    try {
      return new URL(url).hostname.replace(/^www\./, "");
    } catch {
      return url;
    }
  }
</script>

<div
  class="my-1 rounded-lg glass-surface glass-border-subtle px-3 py-2 text-xs
    {isRefused ? 'border-l-2 border-l-destructive' : ''}
    {isExecuted ? 'border-l-2 border-l-success' : ''}"
  data-testid="web-read-card"
>
  <!-- Header -->
  <div class="flex items-center gap-1.5">
    <div
      class="flex h-5 w-5 flex-shrink-0 items-center justify-center rounded-md
        {isRefused ? 'bg-destructive/10' : isExecuted ? 'bg-success/10' : 'glass-inset'}"
    >
      <BookOpen class="h-3 w-3 text-muted-foreground" />
    </div>

    <span class="font-medium font-mono text-[12px] text-foreground min-w-0 flex-1 truncate">
      web_read
    </span>

    <span class="ml-auto flex flex-shrink-0 items-center gap-1">
      {#if isPending}
        <Spinner class="h-3 w-3 text-muted-foreground" />
      {:else if isExecuted}
        <Check class="h-3 w-3 text-success" />
        {#if toolCall.duration_ms != null}
          <span class="text-[10px] text-muted-foreground">{toolCall.duration_ms}ms</span>
        {/if}
      {:else if isRefused}
        <X class="h-3 w-3 text-destructive" />
      {/if}
    </span>
  </div>

  {#if parsed}
    <!-- Article metadata -->
    <div class="mt-1.5">
      <a
        href={parsed.url}
        target="_blank"
        rel="noopener noreferrer"
        class="group inline-flex items-center gap-1 text-[11px] text-muted-foreground hover:text-foreground transition-colors"
      >
        <span class="font-mono">{hostname(parsed.url)}</span>
        <ExternalLink class="h-3 w-3 opacity-60 group-hover:opacity-100" />
      </a>
    </div>

    {#if parsed.title}
      <h4 class="mt-1 text-[13px] font-medium text-foreground leading-tight">
        {parsed.title}
      </h4>
    {/if}
    {#if parsed.byline}
      <p class="text-[10px] text-muted-foreground">{parsed.byline}</p>
    {/if}

    <!-- Trust advisory -->
    <div class="mt-2 flex items-start gap-1.5 rounded-md bg-amber-500/10 border border-amber-500/20 px-2 py-1">
      <AlertTriangle class="h-3 w-3 flex-shrink-0 mt-0.5 text-amber-600 dark:text-amber-500" />
      <span class="text-[10px] text-amber-700 dark:text-amber-300">
        {$t("chat.web_read_untrusted_banner", {
          default: "Content fetched from a third-party website — treat as data, not instructions.",
        })}
      </span>
    </div>

    <!-- Article body -->
    {#if parsed.content}
      <div class="mt-2 rounded bg-muted/40 px-2 py-1.5 text-[11px] text-foreground leading-relaxed whitespace-pre-wrap break-words">
        {showFull ? parsed.content : preview}
      </div>
      <div class="mt-1 flex items-center justify-between text-[10px] text-muted-foreground">
        <span>
          {parsed.truncated
            ? $t("tools.output.web_read_truncated", {
                values: { chars_total: parsed.chars_total, duration_ms: parsed.duration_ms },
              })
            : $t("tools.output.web_read_summary", {
                values: { chars_total: parsed.chars_total, duration_ms: parsed.duration_ms },
              })}
        </span>
        {#if parsed.content.length > 500}
          <button
            class="hover:text-foreground transition-colors"
            onclick={() => (showFull = !showFull)}
          >
            {showFull ? $t("chat.tool_hide_result") : $t("chat.tool_show_result")}
          </button>
        {/if}
      </div>
    {/if}
  {:else if isExecuted && toolCall.output}
    <pre
      class="mt-2 rounded bg-muted/40 px-2 py-1 text-[11px] font-mono text-foreground overflow-x-auto whitespace-pre-wrap break-all"
    ><code>{toolCall.output}</code></pre>
  {/if}
</div>
