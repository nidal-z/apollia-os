<script lang="ts">
  /**
   * Per-tool expanded body for `file_write` and `file_edit`, branched by tool.
   *
   * Operator layer:
   *  - file_write: the path as a chip, a "N lines written" line, and a short
   *    preview of the written content taken from `args.content`.
   *  - file_edit: the path as a chip, a "N replacement(s)" line (from the output
   *    when present, else 1), and a before -> after snippet from the args.
   * Builder layer: the raw args and raw output JSON. Everything degrades to the
   * raw output on a shape mismatch.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { basename, totalLines, prettyJson, stringifyValue } from "$lib/chat/toolBodies";
  import FileRef from "./FileRef.svelte";
  import { t } from "svelte-i18n";
  import { fly, fade } from "svelte/transition";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  interface Props {
    item: ToolCallItem;
    skin: "builder" | "operator";
    /** Session id, so a relative path resolves against the working directory. */
    sessionId?: string;
  }

  let { item, skin, sessionId }: Props = $props();

  const PREVIEW_LINES = 4;
  const SNIPPET_LEN = 160;

  const isEdit = $derived(item.tool === "file_edit");

  const filePath = $derived(
    typeof item.args.path === "string" ? item.args.path : "",
  );
  const fileName = $derived(filePath ? basename(filePath) : "");

  // ---- file_write ----
  const content = $derived(
    typeof item.args.content === "string" ? item.args.content : "",
  );
  const writtenLines = $derived(totalLines(content));
  const previewLines = $derived(
    content.replace(/\n$/, "").split("\n").slice(0, PREVIEW_LINES),
  );
  const previewOverflow = $derived(Math.max(0, writtenLines - PREVIEW_LINES));

  // ---- file_edit ----
  const replacements = $derived.by<number>(() => {
    if (item.output) {
      try {
        const parsed = JSON.parse(item.output) as { replacements?: unknown };
        if (typeof parsed.replacements === "number") return parsed.replacements;
      } catch {
        // fall through
      }
    }
    return 1;
  });
  const oldText = $derived(
    typeof item.args.old_text === "string" ? item.args.old_text : "",
  );
  const newText = $derived(
    typeof item.args.new_text === "string" ? item.args.new_text : "",
  );

  function snippet(s: string): string {
    const one = s.replace(/\s+/g, " ").trim();
    return one.length > SNIPPET_LEN ? one.slice(0, SNIPPET_LEN) + "…" : one;
  }

  const inputJson = $derived(stringifyValue(item.args));
  const outputJson = $derived(prettyJson(item.output));
</script>

<div class="text-[12px]" in:fly={{ y: 8, duration: 200 }}>
  {#key skin}
    <div in:fade={{ duration: 150 }}>
      {#if skin === "operator"}
        <div class="flex flex-col gap-2">
          {#if fileName}
            <span class="tb-chip">
              <span class="tb-chip-key">{$t("tools.body.file_label")}</span>
              <FileRef path={filePath} label={fileName} mono class="tb-chip-val" {sessionId} />
            </span>
          {/if}
          {#if isEdit}
            <p class="tb-metric">
              {$t("tools.body.replacements_count", {
                values: { n: replacements },
              })}
            </p>
            {#if oldText || newText}
              <div class="flex flex-col gap-1.5">
                {#if oldText}
                  <div class="flex flex-col gap-0.5">
                    <span class="tb-iolabel">{$t("tools.body.edit_before_label")}</span>
                    <pre class="tb-preview font-mono"><code class="tb-diff-del">{snippet(oldText)}</code></pre>
                  </div>
                {/if}
                {#if newText}
                  <div class="flex flex-col gap-0.5">
                    <span class="tb-iolabel">{$t("tools.body.edit_after_label")}</span>
                    <pre class="tb-preview font-mono"><code class="tb-diff-add">{snippet(newText)}</code></pre>
                  </div>
                {/if}
              </div>
            {/if}
          {:else}
            <p class="tb-metric">
              {$t("tools.body.lines_written", { values: { n: writtenLines } })}
            </p>
            {#if content}
              <pre class="tb-preview font-mono"><code>{previewLines.join("\n")}{#if previewOverflow > 0}
<span class="tb-more">{$t("tools.body.more_lines", { values: { n: previewOverflow } })}</span>{/if}</code></pre>
            {/if}
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
