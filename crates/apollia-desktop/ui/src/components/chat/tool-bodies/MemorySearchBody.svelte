<script lang="ts">
  /**
   * Per-tool expanded body for `memory_search`.
   *
   * Operator layer: the query as a chip, a "{n} results" lead line, then each
   * recalled entry as a compact card (content excerpt + a meta row with source
   * and relevance when present). Builder layer: the raw output JSON. A parse
   * failure degrades to the raw string. The runtime serializes
   * `{ results: [{ score, source, content, relevance?, created_at }], total_found }`.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { parseMemory, prettyJson } from "$lib/chat/toolBodies";
  import { handleExternalLinkClick } from "$lib/utils/externalLink";
  import { t, locale } from "svelte-i18n";
  import { fly, fade } from "svelte/transition";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  interface Props {
    item: ToolCallItem;
    skin: "builder" | "operator";
  }

  let { item, skin }: Props = $props();

  const query = $derived(
    typeof item.args.query === "string" ? item.args.query : "",
  );
  const parsed = $derived(parseMemory(item.output));
  const rawJson = $derived(prettyJson(item.output));

  function relevanceClamp(value: number): number {
    return Math.round(Math.max(0, Math.min(1, value)) * 100);
  }

  function relevancePct(value: number): string {
    const loc = $locale ?? "en";
    const pct = relevanceClamp(value);
    try {
      return new Intl.NumberFormat(loc).format(pct);
    } catch {
      return String(pct);
    }
  }

  function isUrl(value: string): boolean {
    return /^https?:\/\//i.test(value);
  }
</script>

<div class="text-[12px]" in:fly={{ y: 8, duration: 200 }}>
  {#key skin}
    <div in:fade={{ duration: 150 }}>
      {#if skin === "operator"}
        <div class="flex flex-col gap-2">
          {#if query}
            <span class="tb-chip">
              <span class="tb-chip-key">{$t("tools.body.query_label")}</span>
              <span class="tb-chip-val">{query}</span>
            </span>
          {/if}
          {#if parsed}
            <p class="tb-metric">
              {$t("tools.body.memory_results", {
                values: { n: parsed.totalFound },
              })}
            </p>
            <div class="tb-cards">
              {#each parsed.entries as entry, i (i)}
                {@const rel = entry.relevance ?? entry.score}
                <div class="tb-card">
                  {#if entry.source || rel != null}
                    <div class="tb-card-top">
                      {#if entry.source}
                        {#if isUrl(entry.source)}
                          <a
                            href={entry.source}
                            target="_blank"
                            rel="noopener noreferrer"
                            onclick={handleExternalLinkClick}
                            class="link-inline">{entry.source}</a
                          >
                        {:else}
                          <span>{entry.source}</span>
                        {/if}
                      {/if}
                      {#if rel != null}
                        <span class="tb-mrel"
                          >{relevancePct(rel)}% {$t("tools.body.relevance_label")}</span
                        >
                      {/if}
                    </div>
                  {/if}
                  <p class="tb-card-body">{entry.content}</p>
                  {#if rel != null}
                    <div class="tb-mbar" aria-hidden="true">
                      <span style="width: {relevanceClamp(rel)}%"></span>
                    </div>
                  {/if}
                </div>
              {/each}
            </div>
          {:else if item.output}
            <pre class="tb-code font-mono"><code>{item.output}</code></pre>
          {/if}
        </div>
      {:else}
        <div class="flex flex-col gap-1">
          <div class="tb-iolabel">
            {$t("chat.reasoning.output_label")}
          </div>
          <pre class="tb-code font-mono"><code>{rawJson || item.output}</code></pre>
        </div>
      {/if}
    </div>
  {/key}
</div>
