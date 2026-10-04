<script lang="ts">
  /**
   * Body of a web search step: the query, the number of results, then each
   * result by its title and its site. A title opens the page.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { parseWebSearchOutput } from "$lib/chat/reasoning";
  import { hostOf } from "$lib/chat/toolStep";
  import { handleExternalLinkClick } from "$lib/utils/externalLink";
  import { t } from "svelte-i18n";
  import { Globe } from "lucide-svelte";
  import BodyField from "./BodyField.svelte";
  import BodyRow from "./BodyRow.svelte";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  let { item }: { item: ToolCallItem } = $props();

  /** Results listed at most; the rest are counted, not shown. */
  const SHOWN = 8;

  const parsed = $derived(
    parseWebSearchOutput({ input: item.args, output: item.output, duration_ms: item.duration_ms }),
  );
  const query = $derived(
    parsed?.query ?? (typeof item.args.query === "string" ? item.args.query : ""),
  );
  const results = $derived((parsed?.results ?? []).slice(0, SHOWN));
  const count = $derived(parsed?.total_results ?? parsed?.results.length ?? 0);
</script>

<div class="flex flex-col gap-2" data-testid="tool-body-web_search">
  {#if query}
    <BodyField label={$t("tools.body.search_label")} value={query} />
  {/if}
  {#if parsed}
    <p class="text-caption text-muted-foreground">
      {$t("tools.body.results_count", { values: { n: count } })}
    </p>
    {#each results as result (result.url)}
      <BodyRow icon={Globe} sub={hostOf(result.url)}>
        <a
          href={result.url}
          target="_blank"
          rel="noopener noreferrer"
          onclick={handleExternalLinkClick}
          class="hover:underline">{result.title || hostOf(result.url)}</a
        >
      </BodyRow>
    {/each}
  {/if}
</div>
