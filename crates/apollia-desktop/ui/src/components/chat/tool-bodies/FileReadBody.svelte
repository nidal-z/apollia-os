<script lang="ts">
  /**
   * Body of a file read: the file and the lines read, then the opening lines
   * of its content.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import FileRef from "./FileRef.svelte";
  import BodyField from "./BodyField.svelte";
  import BodyCode from "./BodyCode.svelte";
  import { t } from "svelte-i18n";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  let { item, sessionId }: { item: ToolCallItem; sessionId?: string } = $props();

  /** Lines of the file shown at most. */
  const SHOWN = 12;

  const filePath = $derived(typeof item.args.path === "string" ? item.args.path : "");
  const offset = $derived(typeof item.args.offset === "number" ? item.args.offset : 0);

  const lines = $derived((item.output ?? "").replace(/\n$/, "").split("\n"));
  const total = $derived(item.output ? lines.length : 0);
  const shown = $derived(lines.slice(0, SHOWN).join("\n"));
  const remaining = $derived(Math.max(0, total - SHOWN));

  const range = $derived(
    total > 0
      ? $t("tools.body.lines_range", { values: { from: offset + 1, to: offset + total } })
      : "",
  );
</script>

<div class="flex flex-col gap-2" data-testid="tool-body-file_read">
  {#if filePath}
    <BodyField label={$t("tools.body.file_label")} value={filePath} mono>
      <FileRef path={filePath} mono {sessionId} />{#if range}, {range}{/if}
    </BodyField>
  {/if}
  {#if item.output}
    <BodyCode text={shown} />
    {#if remaining > 0}
      <p class="text-caption text-muted-foreground">
        {$t("tools.body.more_lines", { values: { n: remaining } })}
      </p>
    {/if}
  {/if}
</div>
