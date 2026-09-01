<!--
  Smart output renderer - transforms raw JSON output into formatted views.

  Detects known fields (title, summary, action_items, etc.) and renders them
  with appropriate formatting. Falls back to pre-formatted text for non-JSON.
  Toggle allows switching between formatted and raw views.
-->
<script lang="ts" module>
  /** Render one list item: objects serialize to JSON, never `[object Object]`. */
  function stringifyItem(item: unknown): string {
    if (typeof item === "object" && item !== null) return JSON.stringify(item);
    return String(item);
  }

  /** Coerce a value to a flat string array for list renderers. */
  export function toStringArray(val: unknown): string[] {
    if (Array.isArray(val)) return val.map(stringifyItem);
    if (typeof val === "string") return [val];
    return [stringifyItem(val)];
  }

  /**
   * Format a date string in the given display locale (the caller's
   * `$locale ?? "en"`), returning the original on failure.
   */
  export function formatDateLocal(val: unknown, locale: string): string {
    const str = String(val);
    try {
      const d = new Date(str);
      if (isNaN(d.getTime())) return str;
      return d.toLocaleDateString(locale, {
        year: "numeric",
        month: "long",
        day: "numeric",
      });
    } catch {
      return str;
    }
  }
</script>

<script lang="ts">
  import { t, locale } from "svelte-i18n";
  import { cn } from "$lib/utils";
  import { uiMode } from "$lib/stores/mode";
  import JsonViewer from "./JsonViewer.svelte";
  import MarkdownContent from "$lib/components/ui/markdown/MarkdownContent.svelte";

  interface Props {
    /** Raw output string (JSON or plain text). */
    output: string;
    /** Optional class override. */
    class?: string;
  }

  let { output, class: className = "" }: Props = $props();

  /** Result of attempting to parse the output string as JSON. */
  type ParseResult =
    | { type: "object"; data: Record<string, unknown> }
    | { type: "array"; data: unknown[] }
    | { type: "text"; data: string };

  /** Field renderer type determines how a known field is displayed. */
  type FieldRenderer =
    | "heading"
    | "paragraph"
    | "badge"
    | "date"
    | "inline_list"
    | "figure_list"
    | "checklist"
    | "alert"
    | "error";

  // i18n-ignore-start: renderer identifiers keyed by payload field names, not copy
  const FIELD_RENDERERS: Record<string, FieldRenderer> = {
    title: "heading",
    name: "heading",
    summary: "paragraph",
    executive_summary: "paragraph",
    description: "paragraph",
    category: "badge",
    type: "badge",
    status: "badge",
    date: "date",
    key_entities: "inline_list",
    key_people: "inline_list",
    key_figures: "figure_list",
    action_items: "checklist",
    tasks: "checklist",
    todos: "checklist",
    flags: "alert",
    warnings: "alert",
    alerts: "alert",
    error: "error",
    errors: "error",
  };
  // i18n-ignore-end

  /** Rendering order - known fields first in logical order, then unknowns. */
  const FIELD_ORDER: string[] = [
    "title",
    "name",
    "summary",
    "executive_summary",
    "description",
    "category",
    "type",
    "status",
    "date",
    "key_entities",
    "key_people",
    "key_figures",
    "action_items",
    "tasks",
    "todos",
    "flags",
    "warnings",
    "alerts",
    "error",
    "errors",
  ];

  let showRaw = $state(false);
  let mode = $derived($uiMode);

  const parsed = $derived(tryParseJson(output));
  const isMarkdown = $derived(parsed.type === "text" && looksLikeMarkdown(parsed.data));

  function tryParseJson(str: string): ParseResult {
    try {
      const data: unknown = JSON.parse(str);
      if (Array.isArray(data)) return { type: "array", data };
      if (typeof data === "object" && data !== null)
        return { type: "object", data: data as Record<string, unknown> };
      return { type: "text", data: str };
    } catch {
      return { type: "text", data: str };
    }
  }

  /** Return known field renderer or undefined for unknown fields. */
  function getFieldRenderer(key: string): FieldRenderer | undefined {
    return FIELD_RENDERERS[key];
  }

  /** Sort object keys: known fields in FIELD_ORDER first, then unknown keys alphabetically. */
  function sortedKeys(data: Record<string, unknown>): string[] {
    const keys = Object.keys(data);
    const known: string[] = [];
    const unknown: string[] = [];
    for (const key of keys) {
      if (FIELD_ORDER.includes(key)) {
        known.push(key);
      } else {
        unknown.push(key);
      }
    }
    known.sort((a, b) => FIELD_ORDER.indexOf(a) - FIELD_ORDER.indexOf(b));
    unknown.sort();
    return [...known, ...unknown];
  }

  /** Pretty-print a value for generic key/value display. */
  function formatGenericValue(val: unknown): string {
    if (val === null || val === undefined) return "-";
    if (typeof val === "string") return val;
    if (typeof val === "number" || typeof val === "boolean") return String(val);
    return JSON.stringify(val, null, 2);
  }

  /** Detect column names from an array of objects. */
  function detectColumns(data: unknown[]): string[] {
    const colSet = new Set<string>();
    for (const row of data) {
      if (typeof row === "object" && row !== null && !Array.isArray(row)) {
        for (const key of Object.keys(row)) {
          colSet.add(key);
        }
      }
    }
    return Array.from(colSet);
  }

  /**
   * Heuristic: does this plain-text string look like markdown?
   * Returns true if it contains a fenced code block or at least 2 markdown signals.
   */
  function looksLikeMarkdown(text: string): boolean {
    if (text.length < 20) return false;
    if (/```/.test(text)) return true;
    let signals = 0;
    if (/^#{1,6}\s/m.test(text)) signals++;
    if (/\*\*[^*\n]+\*\*/.test(text)) signals++;
    if (/`[^`\n]+`/.test(text)) signals++;
    if (/^[-*]\s/m.test(text)) signals++;
    if (/^\d+\.\s/m.test(text)) signals++;
    if (/\[.+\]\(.+\)/.test(text)) signals++;
    if (/\n\n/.test(text)) signals++;
    return signals >= 2;
  }

  /** Safely access a property on an unknown row. */
  function cellValue(row: unknown, key: string): string {
    if (typeof row === "object" && row !== null && key in row) {
      const val = (row as Record<string, unknown>)[key];
      if (val === null || val === undefined) return "-";
      if (typeof val === "object") return JSON.stringify(val);
      return String(val);
    }
    return "-";
  }

  /** Humanize a field key for display (snake_case → Title Case). */
  function humanizeKey(key: string): string {
    return key
      .replace(/_/g, " ")
      .replace(/\b\w/g, (c) => c.toUpperCase());
  }
</script>

<div class={cn("space-y-3", className)} data-testid="smart-output">
  {#if showRaw || parsed.type === "text"}
    <!-- Raw / plain text view -->
    {#if parsed.type === "text"}
      {#if isMarkdown && !showRaw}
        <div class="text-sm" data-testid="smart-output-markdown">
          <MarkdownContent content={output} />
        </div>
      {:else}
        <pre
          class="whitespace-pre-wrap rounded glass-border glass-surface p-3 font-mono text-sm"
          data-testid="smart-output-text"
        >{output}</pre>
      {/if}
    {:else}
      <JsonViewer json={output} />
    {/if}
  {:else if parsed.type === "object"}
    <!-- Structured view for JSON objects -->
    <div class="space-y-3" data-testid="smart-output-formatted">
      {#each sortedKeys(parsed.data) as key (key)}
        {@const renderer = getFieldRenderer(key)}
        {@const value = parsed.data[key]}

        {#if renderer === "heading"}
          <h2 class="text-lg font-medium" data-testid="smart-output-heading">
            {String(value)}
          </h2>
        {:else if renderer === "paragraph"}
          <p class="text-sm leading-relaxed text-foreground/90" data-testid="smart-output-paragraph">
            {String(value)}
          </p>
        {:else if renderer === "badge"}
          <div class="flex items-center gap-2">
            <span class="text-xs text-muted-foreground">{humanizeKey(key)}</span>
            <span
              class="inline-flex items-center rounded-full leading-none border border-transparent bg-secondary px-2.5 py-0.5 text-xs font-semibold text-secondary-foreground"
              data-testid="smart-output-badge"
            >
              {String(value)}
            </span>
          </div>
        {:else if renderer === "date"}
          <div class="flex items-center gap-2 text-sm">
            <span class="text-muted-foreground">{humanizeKey(key)}:</span>
            <span data-testid="smart-output-date">{formatDateLocal(value, $locale ?? "en")}</span>
          </div>
        {:else if renderer === "inline_list"}
          <div class="space-y-1" data-testid="smart-output-inline-list">
            <span class="text-xs font-medium text-muted-foreground">{humanizeKey(key)}</span>
            <div class="flex flex-wrap gap-1.5">
              {#each toStringArray(value) as item}
                <span class="inline-flex items-center rounded-md glass-inset px-2 py-0.5 text-xs">
                  {item}
                </span>
              {/each}
            </div>
          </div>
        {:else if renderer === "figure_list"}
          <div class="space-y-1" data-testid="smart-output-figure-list">
            <span class="text-xs font-medium text-muted-foreground">{humanizeKey(key)}</span>
            <ul class="space-y-0.5 text-sm">
              {#each toStringArray(value) as item}
                <li class="flex items-center gap-1.5">
                  <span class="text-muted-foreground">•</span>
                  <span>{item}</span>
                </li>
              {/each}
            </ul>
          </div>
        {:else if renderer === "checklist"}
          <div class="space-y-1" data-testid="smart-output-checklist">
            <span class="text-xs font-medium text-muted-foreground">{humanizeKey(key)}</span>
            <ul class="space-y-1">
              {#each toStringArray(value) as item}
                <li class="flex items-start gap-2 text-sm">
                  <input type="checkbox" disabled class="mt-0.5 rounded border-muted-foreground/40" />
                  <span>{item}</span>
                </li>
              {/each}
            </ul>
          </div>
        {:else if renderer === "alert"}
          <div
            class="rounded-md border border-warning/40 bg-warning/10 px-3 py-2 text-sm text-warning-foreground"
            role="alert"
            data-testid="smart-output-alert"
          >
            <span class="font-semibold text-warning">{humanizeKey(key)}</span>
            <ul class="mt-1 space-y-0.5">
              {#each toStringArray(value) as item}
                <li>- {item}</li>
              {/each}
            </ul>
          </div>
        {:else if renderer === "error"}
          <div
            class="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive"
            role="alert"
            data-testid="smart-output-error"
          >
            <span class="font-semibold text-destructive">{humanizeKey(key)}</span>
            <ul class="mt-1 space-y-0.5">
              {#each toStringArray(value) as item}
                <li>- {item}</li>
              {/each}
            </ul>
          </div>
        {:else}
          <!-- Generic key/value for unknown fields -->
          <div class="text-sm" data-testid="smart-output-generic">
            <span class="text-muted-foreground">{humanizeKey(key)}:</span>
            {#if typeof value === "object" && value !== null}
              <pre class="mt-1 whitespace-pre-wrap rounded glass-inset p-2 font-mono text-xs">{formatGenericValue(value)}</pre>
            {:else}
              <span class="ml-1">{formatGenericValue(value)}</span>
            {/if}
          </div>
        {/if}
      {/each}
    </div>
  {:else if parsed.type === "array"}
    <!-- Table view for JSON arrays -->
    {@const columns = detectColumns(parsed.data)}
    {#if columns.length > 0}
      <div class="overflow-x-auto rounded glass-border glass-surface" data-testid="smart-output-table">
        <table class="w-full text-sm">
          <thead>
            <tr class="border-b glass-border-subtle glass-surface">
              {#each columns as col}
                <th scope="col" class="px-3 py-2 text-left text-xs font-medium text-muted-foreground">
                  {humanizeKey(col)}
                </th>
              {/each}
            </tr>
          </thead>
          <tbody>
            {#each parsed.data as row, i}
              <tr class={cn("border-b last:border-b-0", i % 2 === 1 && "bg-muted/30")}>
                {#each columns as col}
                  <td class="px-3 py-2">{cellValue(row, col)}</td>
                {/each}
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else}
      <!-- Array of primitives - render as list -->
      <pre
        class="whitespace-pre-wrap rounded glass-border glass-surface p-3 font-mono text-sm"
      >{output}</pre>
    {/if}
  {/if}

  <!-- Toggle raw/formatted (for JSON and markdown content) -->
  {#if parsed.type !== "text" || isMarkdown}
    <div class={cn("pt-1", mode === "operator" ? "text-right" : "")}>
      <button
        class={cn(
          "text-xs transition-colors",
          mode === "operator"
            ? "text-muted-foreground/60 hover:text-muted-foreground"
            : "rounded border px-2 py-1 text-muted-foreground hover:glass-inset"
        )}
        onclick={() => (showRaw = !showRaw)}
        data-testid="smart-output-raw-toggle"
      >
        {showRaw ? $t("smart_output.show_formatted") : $t("smart_output.show_raw")}
      </button>
    </div>
  {/if}
</div>
