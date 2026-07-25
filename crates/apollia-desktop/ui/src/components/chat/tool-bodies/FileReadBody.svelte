<script lang="ts">
  /**
   * Per-tool expanded body for `file_read`.
   *
   * Operator layer: the file name as a chip plus a short muted preview (first
   * few lines) with a "N more lines" hint. Builder layer: the full content with
   * left-aligned line numbers, capped so a huge file cannot flood the DOM.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { basename } from "$lib/chat/toolBodies";
  import FileRef from "./FileRef.svelte";
  import { t } from "svelte-i18n";
  import { fly, fade } from "svelte/transition";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  interface Props {
    item: ToolCallItem;
    skin: "builder" | "operator";
  }

  let { item, skin }: Props = $props();

  const PREVIEW_LINES = 4;
  const BUILDER_MAX_LINES = 500;

  const filePath = $derived(
    typeof item.args.path === "string" ? item.args.path : "",
  );
  const fileName = $derived(filePath ? basename(filePath) : "");

  const lines = $derived((item.output ?? "").replace(/\n$/, "").split("\n"));
  const totalLines = $derived(item.output ? lines.length : 0);

  const previewLines = $derived(lines.slice(0, PREVIEW_LINES));
  const remaining = $derived(Math.max(0, totalLines - PREVIEW_LINES));

  const builderLines = $derived(lines.slice(0, BUILDER_MAX_LINES));
  const builderOverflow = $derived(Math.max(0, totalLines - BUILDER_MAX_LINES));
</script>

<div class="text-[12px]" in:fly={{ y: 8, duration: 200 }}>
  {#key skin}
    <div in:fade={{ duration: 150 }}>
      {#if skin === "operator"}
        <div class="flex flex-col gap-2">
          {#if fileName}
            <span class="tb-chip">
              <span class="tb-chip-key">{$t("tools.body.file_label")}</span>
              <FileRef path={filePath} label={fileName} mono class="tb-chip-val" />
            </span>
          {/if}
          {#if item.output}
            <pre class="tb-preview font-mono"><code>{previewLines.join("\n")}{#if remaining > 0}
<span class="tb-more">{$t("tools.body.more_lines", { values: { n: remaining } })}</span>{/if}</code></pre>
          {/if}
        </div>
      {:else}
        <div class="flex flex-col gap-1">
          <div class="tb-iolabel">{$t("tools.body.raw_content")}</div>
          <pre class="tb-preview font-mono"><code>{#each builderLines as line, i}<span class="tb-ln">{i + 1}</span>{line}{"\n"}{/each}</code>{#if builderOverflow > 0}<span class="tb-more">{$t("tools.body.more_lines", { values: { n: builderOverflow } })}</span>{/if}</pre>
        </div>
      {/if}
    </div>
  {/key}
</div>
