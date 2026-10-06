<script lang="ts">
  /**
   * Body of a file search by name: the pattern, how many files matched, then
   * each file by its path. A path reveals the file.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { parseFileGlob } from "$lib/chat/toolBodies";
  import { fileIcon } from "$lib/chat/toolBodyFormat";
  import FileRef from "./FileRef.svelte";
  import BodyField from "./BodyField.svelte";
  import BodyRow from "./BodyRow.svelte";
  import { t } from "svelte-i18n";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  let { item, sessionId }: { item: ToolCallItem; sessionId?: string } = $props();

  /** Files listed at most. */
  const SHOWN = 20;

  const pattern = $derived(typeof item.args.pattern === "string" ? item.args.pattern : "");
  const matches = $derived(parseFileGlob(item.output));
</script>

<div class="flex flex-col gap-2" data-testid="tool-body-file_glob">
  {#if pattern}
    <BodyField label={$t("tools.body.pattern_label")} value={pattern} mono />
  {/if}
  {#if matches}
    <p class="text-caption text-muted-foreground">
      {$t("tools.body.files_found", { values: { n: matches.length } })}
    </p>
    {#each matches.slice(0, SHOWN) as path (path)}
      <BodyRow icon={fileIcon(path)}>
        <FileRef {path} {sessionId} />
      </BodyRow>
    {/each}
  {:else if item.output}
    <p class="text-body-xs text-muted-foreground">{item.output.slice(0, 400)}</p>
  {/if}
</div>
