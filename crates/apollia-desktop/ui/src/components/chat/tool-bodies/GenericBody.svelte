<script lang="ts">
  /**
   * Body of a step whose tool has no body of its own (connections, external
   * tools): the parameters it was given, then what came back, read as a
   * sentence rather than shown as data.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { stepParameters, toolCallView } from "$lib/chat/toolStep";
  import {
    resolveToolDisplay,
    buildOutputSummary,
    summariseJsonOutput,
  } from "$lib/tools/tool-display";
  import { t } from "svelte-i18n";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  let { item }: { item: ToolCallItem } = $props();

  const params = $derived(stepParameters(item.args));

  const result = $derived.by<string | null>(() => {
    if (item.status !== "success" && item.status !== "approved") return null;
    const display = resolveToolDisplay(toolCallView(item));
    if (display.outputSummaryKey && Object.keys(display.outputParams ?? {}).length > 0) {
      return $t(display.outputSummaryKey, { values: display.outputParams });
    }
    const technical = buildOutputSummary(item.tool, display.outputParams);
    if (technical) return technical;
    if (typeof item.output === "string" && item.output.length > 0) {
      const shape = summariseJsonOutput(item.output);
      if (shape === "empty") return $t("chat.tool_empty_results");
      if (shape !== null) return $t("tools.body.results_count", { values: { n: Number(shape) } });
      return (item.output.split("\n")[0] ?? "").slice(0, 200);
    }
    return null;
  });
</script>

<div class="flex flex-col gap-2" data-testid="tool-body-generic">
  {#if params.length > 0}
    <p class="text-caption text-muted-foreground">{$t("tools.body.params_label")}</p>
    {#each params as [key, value] (key)}
      <div class="flex min-w-0 items-baseline gap-2">
        <span class="flex-none text-caption text-muted-foreground">{key.replace(/_/g, " ")}</span>
        <span class="min-w-0 truncate font-mono text-body-xs text-foreground" title={value}
          >{value}</span
        >
      </div>
    {/each}
  {/if}
  {#if result}
    <p class="text-caption text-muted-foreground">{$t("chat.reasoning.result_label")}</p>
    <p class="text-body-xs text-foreground">{result}</p>
  {/if}
</div>
