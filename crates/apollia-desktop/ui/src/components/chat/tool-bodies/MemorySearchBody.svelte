<script lang="ts">
  /**
   * Body of a memory search: what was looked for, then each memory found and
   * the day it was kept.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { parseMemory } from "$lib/chat/toolBodies";
  import { formatDay } from "$lib/chat/toolBodyFormat";
  import BodyField from "./BodyField.svelte";
  import BodyRow from "./BodyRow.svelte";
  import { t, locale } from "svelte-i18n";
  import { Database } from "lucide-svelte";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  let { item }: { item: ToolCallItem } = $props();

  const query = $derived(typeof item.args.query === "string" ? item.args.query : "");
  const parsed = $derived(parseMemory(item.output));

  function kept(createdAt: string | null): string {
    const day = formatDay(createdAt, $locale ?? "en");
    return day ? $t("tools.body.remembered_on", { values: { date: day } }) : "";
  }
</script>

<div class="flex flex-col gap-2" data-testid="tool-body-memory_search">
  {#if query}
    <BodyField label={$t("tools.body.search_label")} value={query} />
  {/if}
  {#if parsed}
    {#if parsed.entries.length === 0}
      <p class="text-caption text-muted-foreground">{$t("chat.tool_empty_results")}</p>
    {/if}
    {#each parsed.entries as entry, i (i)}
      <BodyRow icon={Database} title={entry.content} sub={kept(entry.createdAt)} />
    {/each}
  {:else if item.output}
    <p class="text-body-xs text-muted-foreground">{item.output.slice(0, 400)}</p>
  {/if}
</div>
