<script lang="ts">
  /**
   * Body of a Python run: the code, then what it printed as its result, or
   * the error it raised.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { parseBashOutput } from "$lib/chat/toolBodies";
  import BodyField from "./BodyField.svelte";
  import BodyCode from "./BodyCode.svelte";
  import { t } from "svelte-i18n";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  let { item }: { item: ToolCallItem } = $props();

  /** Lines of code or output shown at most. */
  const SHOWN = 20;

  const code = $derived(typeof item.args.code === "string" ? item.args.code : "");
  const parsed = $derived(parseBashOutput(item.output, item.exit_code));

  function clip(text: string): string {
    return text.replace(/\n+$/, "").split("\n").slice(0, SHOWN).join("\n");
  }

  const result = $derived(clip(parsed.body));
  const singleLine = $derived(result.length > 0 && !result.includes("\n"));
</script>

<div class="flex flex-col gap-2" data-testid="tool-body-python_executor">
  {#if code}
    <BodyCode text={clip(code)} />
  {/if}
  {#if result}
    {#if singleLine}
      <BodyField label={$t("chat.reasoning.result_label")} value={result} mono />
    {:else}
      <p class="text-caption text-muted-foreground">{$t("chat.reasoning.result_label")}</p>
      <BodyCode text={result} />
    {/if}
  {/if}
  {#if parsed.stderr}
    <p class="whitespace-pre-wrap break-words font-mono text-body-xs text-destructive">
      {clip(parsed.stderr)}
    </p>
  {/if}
</div>
