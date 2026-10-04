<script lang="ts">
  /**
   * Body of a search inside files: the words searched, how many passages in
   * how many files, then each passage by its file and line, with its text.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { parseGrep, basename } from "$lib/chat/toolBodies";
  import FileRef from "./FileRef.svelte";
  import BodyField from "./BodyField.svelte";
  import BodyRow from "./BodyRow.svelte";
  import { t } from "svelte-i18n";
  import { FileText } from "lucide-svelte";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  let { item, sessionId }: { item: ToolCallItem; sessionId?: string } = $props();

  /** Passages listed at most. */
  const SHOWN = 20;

  const pattern = $derived(typeof item.args.pattern === "string" ? item.args.pattern : "");
  const parsed = $derived(parseGrep(item.output));
  const passages = $derived(
    (parsed?.groups ?? [])
      .flatMap((g) => g.rows.map((row) => ({ file: g.file, line: row.line, text: row.text.trim() })))
      .slice(0, SHOWN),
  );
  const fileCount = $derived(parsed?.groups.length ?? 0);
</script>

<div class="flex flex-col gap-2" data-testid="tool-body-file_grep">
  {#if pattern}
    <BodyField label={$t("tools.body.searched_words_label")} value={pattern} mono />
  {/if}
  {#if parsed}
    <p class="text-caption text-muted-foreground">
      {$t("tools.body.grep_found", { values: { m: parsed.totalMatches, f: fileCount } })}{#if parsed.truncated}, {$t("tools.body.truncated_hint")}{/if}
    </p>
    {#each passages as passage, i (i)}
      <BodyRow icon={FileText} sub={passage.text}>
        <FileRef
          path={passage.file}
          label={$t("tools.body.file_line", { values: { file: basename(passage.file), n: passage.line } })}
          {sessionId}
        />
      </BodyRow>
    {/each}
  {:else if item.output}
    <p class="text-body-xs text-muted-foreground">{item.output.slice(0, 400)}</p>
  {/if}
</div>
