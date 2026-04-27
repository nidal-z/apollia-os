<script lang="ts">
  /**
   * Filesystem diff preview with create/update/delete/chmod support
   * Diff lines render git-style: red for removals, green
   * for additions; neutral for context.
   */
  import { t } from "svelte-i18n";
  import { FilePlus, FileMinus, FileEdit, Lock } from "lucide-svelte";
  import type { FilesystemPreview, FilesystemChange } from "./types";

  interface Props {
    payload: FilesystemPreview;
    /** Hard cap in lines per file before the "Show more" toggle kicks in. */
    limit?: number;
  }

  let { payload, limit = 200 }: Props = $props();

  const expanded: Record<string, boolean> = $state({});

  function iconFor(action: FilesystemChange["action"]) {
    switch (action) {
      case "create": return FilePlus;
      case "delete": return FileMinus;
      case "chmod": return Lock;
      default: return FileEdit;
    }
  }

  function actionLabel(action: FilesystemChange["action"]): string {
    return $t(`permissions.preview.fs.action.${action}`);
  }

  function diffLines(diff: string): string[] {
    return diff.split("\n");
  }

  function lineClass(line: string): string {
    if (line.startsWith("+")) return "bg-success/10 text-success-foreground";
    if (line.startsWith("-")) return "bg-destructive/10 text-destructive";
    if (line.startsWith("@")) return "text-muted-foreground";
    return "text-foreground";
  }
</script>

<div class="space-y-3" data-testid="preview-filesystem">
  {#each payload.changes as change (change.path)}
    {@const Icon = iconFor(change.action)}
    {@const allLines = change.diff ? diffLines(change.diff) : []}
    {@const showAll = expanded[change.path]}
    {@const visible = showAll ? allLines : allLines.slice(0, limit)}
    {@const truncated = allLines.length - limit}
    <div class="rounded-md border border-border">
      <header class="flex items-center gap-2 border-b border-border px-3 py-2 text-xs">
        <Icon size={14} />
        <span class="font-semibold uppercase tracking-wide">{actionLabel(change.action)}</span>
        <code class="min-w-0 flex-1 truncate font-mono text-[11px]">{change.path}</code>
        {#if change.action === "chmod"}
          <span class="font-mono text-[11px] text-muted-foreground">
            {change.old_mode ?? "?"} → {change.new_mode ?? "?"}
          </span>
        {/if}
      </header>

      {#if change.diff}
        <pre class="max-h-[480px] overflow-auto bg-muted/40 text-[11px] leading-relaxed"
          data-testid="preview-filesystem-diff"
        >{#each visible as line (line)}<div class={lineClass(line) + " px-3"}>{line || " "}</div>{/each}</pre>

        {#if truncated > 0 && !showAll}
          <button
            class="w-full border-t border-border px-3 py-1.5 text-xs text-primary hover:bg-muted"
            onclick={() => (expanded[change.path] = true)}
            data-testid="preview-filesystem-show-more"
          >
            {$t("permissions.preview.fs.show_more", { values: { count: truncated } })}
          </button>
        {/if}
      {/if}
    </div>
  {/each}
</div>
