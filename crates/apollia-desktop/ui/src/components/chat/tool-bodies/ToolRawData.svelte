<script lang="ts">
  /**
   * The raw data of a step, shown only in developer mode: the tool's
   * identifier, its arguments and its output as they travelled, long ones
   * cut with a way to see them whole.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { JSON_LINE_THRESHOLD } from "$lib/chat/reasoning";
  import { prettyJson, stringifyValue } from "$lib/chat/toolBodies";
  import { t } from "svelte-i18n";
  import BodyCode from "./BodyCode.svelte";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  let { item }: { item: ToolCallItem } = $props();

  let fullInput = $state(false);
  let fullOutput = $state(false);

  function cut(raw: string): { preview: string; cut: boolean } {
    const lines = raw.split("\n");
    if (lines.length <= JSON_LINE_THRESHOLD) return { preview: raw, cut: false };
    return { preview: `${lines.slice(0, JSON_LINE_THRESHOLD).join("\n")}\n…`, cut: true };
  }

  const input = $derived(stringifyValue(item.args));
  const output = $derived(prettyJson(item.output) || (item.output ?? ""));
  const inputCut = $derived(cut(input));
  const outputCut = $derived(cut(output));
</script>

<div class="flex flex-col gap-2 border-t border-border/60 pt-2" data-testid="tool-step-raw">
  <p class="text-caption text-muted-foreground">
    {$t("chat.step.raw_label")} · <span class="font-mono">{item.tool}</span>
  </p>
  <p class="text-caption text-muted-foreground">{$t("chat.reasoning.input_label")}</p>
  <BodyCode text={fullInput ? input : inputCut.preview} />
  {#if inputCut.cut}
    <button
      type="button"
      class="self-start text-caption text-muted-foreground hover:text-foreground"
      data-testid="tool-step-raw-input-all"
      onclick={(e) => {
        e.stopPropagation();
        fullInput = !fullInput;
      }}>{fullInput ? $t("chat.tool_collapse") : $t("chat.reasoning.see_all")}</button
    >
  {/if}
  {#if output}
    <p class="text-caption {item.status === 'error' ? 'text-destructive' : 'text-muted-foreground'}">
      {$t("chat.reasoning.output_label")}
    </p>
    <BodyCode text={fullOutput ? output : outputCut.preview} />
    {#if outputCut.cut}
      <button
        type="button"
        class="self-start text-caption text-muted-foreground hover:text-foreground"
        data-testid="tool-step-raw-output-all"
        onclick={(e) => {
          e.stopPropagation();
          fullOutput = !fullOutput;
        }}>{fullOutput ? $t("chat.tool_collapse") : $t("chat.reasoning.see_all")}</button
      >
    {/if}
  {/if}
</div>
