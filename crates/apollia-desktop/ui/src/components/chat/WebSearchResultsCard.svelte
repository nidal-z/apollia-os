<script lang="ts">
  /**
   * Rich renderer for `web_search` tool output.
   *
   * Replaces the generic JSON dump of `BuilderToolCard` with clickable result
   * cards (title + hostname + snippet). Used in builder mode for readability
   * and in operator mode when the result is completed; falls back to the raw
   * JSON (via the parent) if the output is not parseable.
   *
   * Security note (ADR-072): `web_search` does not hit `web_read` internally,
   * so the snippets here are attacker-adjacent but lower-risk than
   * `web_read` content. We still treat every field as untrusted and render
   * as plain text via Svelte's automatic escaping.
   */

  import type { ToolCallView } from "$lib/types";
  import { Check, X, ExternalLink, Compass } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import { t } from "svelte-i18n";

  let { toolCall }: { toolCall: ToolCallView } = $props();

  interface WebSearchResult {
    title: string;
    url: string;
    snippet: string;
    rank: number;
    age: string | null;
  }

  interface WebSearchOutput {
    query: string;
    backend: string;
    results: WebSearchResult[];
    total_results: number;
    duration_ms: number;
  }

  const query = $derived(
    typeof toolCall.input.query === "string" ? toolCall.input.query : "",
  );

  const parsed: WebSearchOutput | null = $derived.by(() => {
    if (!toolCall.output) return null;
    try {
      const v = JSON.parse(toolCall.output) as WebSearchOutput;
      if (!v || !Array.isArray(v.results)) return null;
      return v;
    } catch {
      return null;
    }
  });

  const isPending = $derived(
    toolCall.status === "pending" || toolCall.status === "authorized",
  );
  const isRefused = $derived(toolCall.status === "refused");
  const isExecuted = $derived(toolCall.status === "executed");

  /** Turn a full URL into a short display hostname like `github.com`. */
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
  data-testid="web-search-card"
>
  <!-- Header -->
  <div class="flex items-center gap-1.5">
    <div
      class="flex h-5 w-5 flex-shrink-0 items-center justify-center rounded-md
        {isRefused ? 'bg-destructive/10' : isExecuted ? 'bg-success/10' : 'glass-inset'}"
    >
      <Compass class="h-3 w-3 text-muted-foreground" />
    </div>

    <span class="font-medium font-mono text-[12px] text-foreground min-w-0 flex-1 truncate">
      web_search
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

  <!-- Query line -->
  {#if query}
    <p class="mt-1.5 text-[11px] text-muted-foreground">
      <span class="font-mono">“{query}”</span>
      {#if parsed}
        <span class="mx-1">·</span>
        <span>{parsed.backend}</span>
        <span class="mx-1">·</span>
        <span>{$t("tools.output.web_search_summary", { values: {
          total_results: parsed.total_results,
          backend: parsed.backend,
          duration_ms: parsed.duration_ms
        } })}</span>
      {/if}
    </p>
  {/if}

  <!-- Result list -->
  {#if parsed && parsed.results.length > 0}
    <ol class="mt-2 space-y-2">
      {#each parsed.results as r (r.rank)}
        <li class="rounded-md glass-inset p-2">
          <a
            href={r.url}
            target="_blank"
            rel="noopener noreferrer"
            class="group block"
          >
            <div class="flex items-start justify-between gap-2">
              <span class="text-[12px] font-medium text-foreground group-hover:underline line-clamp-2">
                {r.title}
              </span>
              <ExternalLink class="mt-0.5 h-3 w-3 flex-shrink-0 text-muted-foreground opacity-0 group-hover:opacity-100 transition-opacity" />
            </div>
            <div class="mt-0.5 flex items-center gap-1 text-[10px] text-muted-foreground">
              <span class="font-mono">{hostname(r.url)}</span>
              {#if r.age}
                <span>·</span>
                <span>{r.age}</span>
              {/if}
              <span class="ml-auto">#{r.rank}</span>
            </div>
            {#if r.snippet}
              <p class="mt-1 text-[11px] text-muted-foreground line-clamp-3">
                {r.snippet}
              </p>
            {/if}
          </a>
        </li>
      {/each}
    </ol>
  {:else if parsed && parsed.results.length === 0}
    <p class="mt-2 text-[11px] italic text-muted-foreground">
      {$t("chat.tool_empty_results", { default: "No results." })}
    </p>
  {:else if isExecuted && toolCall.output}
    <!-- Fallback: output present but unparseable -->
    <pre
      class="mt-2 rounded bg-muted/40 px-2 py-1 text-[11px] font-mono text-foreground overflow-x-auto whitespace-pre-wrap break-all"
    ><code>{toolCall.output}</code></pre>
  {/if}
</div>
