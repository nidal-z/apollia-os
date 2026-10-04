<script lang="ts">
  /**
   * Body of a folder listing: the folder, then each entry with what it holds
   * (a folder its item count, a file its size). A name reveals the entry.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { parseFileList, isFolder, humanSize, type FileListEntry } from "$lib/chat/toolBodies";
  import { fileIcon, formatSize } from "$lib/chat/toolBodyFormat";
  import { joinPath } from "$lib/utils/fileRef";
  import FileRef from "./FileRef.svelte";
  import BodyField from "./BodyField.svelte";
  import BodyRow from "./BodyRow.svelte";
  import { t, locale } from "svelte-i18n";
  import { Folder } from "lucide-svelte";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  let { item, sessionId }: { item: ToolCallItem; sessionId?: string } = $props();

  const path = $derived(
    typeof item.args.dir === "string"
      ? item.args.dir
      : typeof item.args.path === "string"
        ? item.args.path
        : "",
  );
  const entries = $derived(parseFileList(item.output));

  function sub(entry: FileListEntry): string {
    if (isFolder(entry)) {
      return $t("tools.body.entries_count", { values: { n: entry.entries ?? 0 } });
    }
    if (typeof entry.size === "number") {
      const size = humanSize(entry.size);
      return formatSize(size.value, $t(`tools.body.unit_${size.unit}`), $locale ?? "en");
    }
    return "";
  }
</script>

<div class="flex flex-col gap-2" data-testid="tool-body-file_list">
  {#if path}
    <BodyField label={$t("tools.body.folder_label")} value={path} mono>
      <FileRef {path} mono {sessionId} />
    </BodyField>
  {/if}
  {#if entries}
    {#each entries as entry (entry.name)}
      <BodyRow icon={isFolder(entry) ? Folder : fileIcon(entry.name)} sub={sub(entry)}>
        <FileRef path={joinPath(path, entry.name)} label={entry.name} {sessionId} />
      </BodyRow>
    {/each}
  {:else if item.output}
    <p class="text-body-xs text-muted-foreground">{item.output.slice(0, 400)}</p>
  {/if}
</div>
