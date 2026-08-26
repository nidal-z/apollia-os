<script lang="ts">
  /**
   * Per-tool expanded body for `file_list` (the "pitcher").
   *
   * Operator layer: the target directory as a chip, then a clean tabular
   * listing (folder/file icon + name + size-or-entry-count) parsed from the raw
   * output. Builder layer: the raw input and output JSON. A parse failure
   * degrades to the raw output string so nothing is ever lost.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import {
    parseFileList,
    isFolder,
    humanSize,
    prettyJson,
    type FileListEntry,
    type HumanSize,
  } from "$lib/chat/toolBodies";
  import { joinPath } from "$lib/utils/fileRef";
  import FileRef from "./FileRef.svelte";
  import { t, locale } from "svelte-i18n";
  import { fly, fade } from "svelte/transition";
  import { Folder, FileText } from "lucide-svelte";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  interface Props {
    item: ToolCallItem;
    skin: "builder" | "operator";
    /** Session id, so a relative path resolves against the working directory. */
    sessionId?: string;
  }

  let { item, skin, sessionId }: Props = $props();

  const path = $derived(
    typeof item.args.dir === "string"
      ? item.args.dir
      : typeof item.args.path === "string"
        ? item.args.path
        : "",
  );

  const entries = $derived(parseFileList(item.output));
  const rawJson = $derived(prettyJson(item.output));
  const inputJson = $derived(JSON.stringify(item.args, null, 2));

  function formatSize(size: HumanSize): string {
    const loc = $locale ?? "en";
    let n: string;
    try {
      n = new Intl.NumberFormat(loc, { maximumFractionDigits: 1 }).format(
        size.value,
      );
    } catch {
      n = String(size.value);
    }
    return `${n} ${$t(`tools.body.unit_${size.unit}`)}`;
  }

  function metaLabel(entry: FileListEntry): string {
    if (isFolder(entry)) {
      const count = entry.entries ?? 0;
      return $t("tools.body.entries_count", { values: { n: count } });
    }
    if (typeof entry.size === "number") {
      return formatSize(humanSize(entry.size));
    }
    return "";
  }
</script>

<div class="text-body-xs" in:fly={{ y: 8, duration: 200 }}>
  {#key skin}
    <div in:fade={{ duration: 150 }}>
      {#if skin === "operator"}
        <div class="flex flex-col gap-2">
          {#if path}
            <span class="tb-chip">
              <span class="tb-chip-key">{$t("tools.body.folder_label")}</span>
              <FileRef {path} mono class="tb-chip-val" {sessionId} />
            </span>
          {/if}
          {#if entries}
            <div class="tb-flist">
              {#each entries as entry (entry.name)}
                <div class="tb-frow">
                  <span
                    class="tb-fi {isFolder(entry) ? 'tb-folder' : ''}"
                    aria-hidden="true"
                  >
                    {#if isFolder(entry)}
                      <Folder size={15} />
                    {:else}
                      <FileText size={14} />
                    {/if}
                  </span>
                  <FileRef
                    path={joinPath(path, entry.name)}
                    label={entry.name}
                    class="tb-fname"
                    {sessionId}
                  />
                  {#if metaLabel(entry)}
                    <span class="tb-fmeta">{metaLabel(entry)}</span>
                  {/if}
                </div>
              {/each}
            </div>
          {:else if item.output}
            <pre class="tb-code font-mono"><code>{item.output}</code></pre>
          {/if}
        </div>
      {:else}
        <div class="flex flex-col gap-2">
          <div class="flex flex-col gap-1">
            <div class="tb-iolabel">
              {$t("chat.reasoning.input_label")}
            </div>
            <pre class="tb-code font-mono"><code>{inputJson}</code></pre>
          </div>
          <div class="flex flex-col gap-1">
            <div class="tb-iolabel">
              {$t("chat.reasoning.output_label")}
            </div>
            <pre class="tb-code font-mono"><code>{rawJson || item.output}</code></pre>
          </div>
        </div>
      {/if}
    </div>
  {/key}
</div>
