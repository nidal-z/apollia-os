<script lang="ts">
  /**
   * Per-tool expanded body for `web_search`.
   *
   * Operator layer: the query as a chip plus a compact "N results" line. The
   * rich, clickable result cards are deliberately deferred to the message-level
   * `SourceCards` at the end of the turn, so the trace stays calm.
   * Builder layer: the raw input (query) and output (results) as pretty JSON.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { parseWebSearchOutput } from "$lib/chat/reasoning";
  import { stringifyValue, prettyJson } from "$lib/chat/toolBodies";
  import { t } from "svelte-i18n";
  import { fly, fade } from "svelte/transition";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  interface Props {
    item: ToolCallItem;
    skin: "builder" | "operator";
  }

  let { item, skin }: Props = $props();

  const parsed = $derived(
    parseWebSearchOutput({
      input: item.args,
      output: item.output,
      duration_ms: item.duration_ms,
    }),
  );

  const query = $derived(
    parsed?.query ??
      (typeof item.args.query === "string" ? item.args.query : ""),
  );
  const resultCount = $derived(
    parsed?.total_results ?? parsed?.results.length ?? 0,
  );

  const inputJson = $derived(stringifyValue(item.args));
  const outputJson = $derived(prettyJson(item.output));
</script>

<div class="text-[12px]" in:fly={{ y: 8, duration: 200 }}>
  {#key skin}
    <div in:fade={{ duration: 150 }}>
      {#if skin === "operator"}
        <div class="flex flex-col gap-2">
          {#if query}
            <span class="tb-chip">
              <span class="tb-chip-key">{$t("tools.body.search_label")}</span>
              <span class="tb-chip-val">{query}</span>
            </span>
          {/if}
          {#if parsed}
            <p class="tb-metric">
              {$t("tools.body.results_count", {
                values: { n: resultCount },
              })}
            </p>
          {/if}
        </div>
      {:else}
        <div class="flex flex-col gap-2">
          <div class="flex flex-col gap-1">
            <div class="tb-iolabel">
              {$t("chat.reasoning.input_label", { default: "Input" })}
            </div>
            <pre class="tb-code font-mono"><code>{inputJson}</code></pre>
          </div>
          {#if outputJson}
            <div class="flex flex-col gap-1">
              <div class="tb-iolabel">
                {$t("chat.reasoning.output_label", { default: "Output" })}
              </div>
              <pre class="tb-code font-mono"><code>{outputJson}</code></pre>
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/key}
</div>
