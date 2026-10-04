<script lang="ts">
  /**
   * Body of a file written or edited: the file, then what changed. An edit
   * shows the removed and added lines; a write shows the opening of the text.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { totalLines } from "$lib/chat/toolBodies";
  import { editLines } from "$lib/chat/toolBodyFormat";
  import FileRef from "./FileRef.svelte";
  import BodyField from "./BodyField.svelte";
  import BodyCode from "./BodyCode.svelte";
  import { t } from "svelte-i18n";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  let { item, sessionId }: { item: ToolCallItem; sessionId?: string } = $props();

  /** Lines of text or of change shown at most. */
  const SHOWN = 12;

  const isEdit = $derived(item.tool === "file_edit");
  const filePath = $derived(typeof item.args.path === "string" ? item.args.path : "");

  const content = $derived(typeof item.args.content === "string" ? item.args.content : "");
  const written = $derived(totalLines(content));
  const preview = $derived(content.replace(/\n$/, "").split("\n").slice(0, SHOWN).join("\n"));

  const diff = $derived(
    editLines(
      typeof item.args.old_text === "string" ? item.args.old_text : "",
      typeof item.args.new_text === "string" ? item.args.new_text : "",
    ),
  );
  const added = $derived(diff.filter((l) => l.sign === "+").length);
  const removed = $derived(diff.filter((l) => l.sign === "-").length);
</script>

<div class="flex flex-col gap-2" data-testid="tool-body-{item.tool}">
  {#if filePath}
    <BodyField label={$t("tools.body.file_label")} value={filePath} mono>
      <FileRef path={filePath} mono {sessionId} />
    </BodyField>
  {/if}
  {#if isEdit}
    {#if diff.length > 0}
      <p class="text-caption text-muted-foreground">
        {$t("tools.body.edit_summary", { values: { added, removed } })}
      </p>
      <BodyCode>
        {#each diff.slice(0, SHOWN) as line, i (i)}<span
            class={line.sign === "-" ? "text-destructive" : "text-success"}
            >{line.sign} {line.text}</span
          >{"\n"}{/each}
      </BodyCode>
    {/if}
  {:else}
    <p class="text-caption text-muted-foreground">
      {$t("tools.body.lines_written", { values: { n: written } })}
    </p>
    {#if content}
      <BodyCode text={preview} />
    {/if}
  {/if}
</div>
