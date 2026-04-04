<script lang="ts">
  import { t } from "svelte-i18n";
  import type { FileEditPermission } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import PermissionActionBar from "./PermissionActionBar.svelte";

  interface Props {
    taskId: string;
    permission: FileEditPermission;
    onResolved: () => void;
  }

  let { taskId, permission, onResolved }: Props = $props();

  type DiffLine = { type: "context" | "removed" | "added"; line: string };

  const MAX_DIFF_LINES = 200;

  function computeLineDiff(oldText: string, newText: string): DiffLine[] {
    const a = oldText.split("\n").slice(0, MAX_DIFF_LINES);
    const b = newText.split("\n").slice(0, MAX_DIFF_LINES);
    const m = a.length;
    const n = b.length;

    const dp: number[][] = Array.from({ length: m + 1 }, () =>
      new Array<number>(n + 1).fill(0),
    );

    for (let i = 1; i <= m; i++) {
      for (let j = 1; j <= n; j++) {
        if (a[i - 1] === b[j - 1]) {
          dp[i][j] = (dp[i - 1]?.[j - 1] ?? 0) + 1;
        } else {
          dp[i][j] = Math.max(dp[i - 1]?.[j] ?? 0, dp[i][j - 1] ?? 0);
        }
      }
    }

    const result: DiffLine[] = [];
    let i = m;
    let j = n;
    while (i > 0 || j > 0) {
      if (i > 0 && j > 0 && a[i - 1] === b[j - 1]) {
        result.push({ type: "context", line: a[i - 1]! });
        i--;
        j--;
      } else if (
        j > 0 &&
        (i === 0 || (dp[i][j - 1] ?? 0) >= (dp[i - 1]?.[j] ?? 0))
      ) {
        result.push({ type: "added", line: b[j - 1]! });
        j--;
      } else {
        result.push({ type: "removed", line: a[i - 1]! });
        i--;
      }
    }

    return result.reverse();
  }

  const diff = $derived(computeLineDiff(permission.old_content, permission.new_content));
</script>

<div class="space-y-2" data-testid="file-edit-permission-view">
  <Badge variant="info">{$t("permissions.type_file_edit")}</Badge>

  <p class="text-[11px] text-muted-foreground">
    <span class="font-medium">{$t("permissions.file_label")}:</span>
    {permission.file_path}
  </p>

  <div class="max-h-64 overflow-y-auto rounded glass-border glass-inset">
    <table class="w-full font-mono text-[12px]">
      <tbody>
        {#each diff as row, idx (idx)}
          {#if row.type === "removed"}
            <tr class="bg-red-950/20">
              <td class="w-4 select-none px-1.5 text-right text-red-400">-</td>
              <td class="whitespace-pre px-1.5 text-red-300">{row.line}</td>
            </tr>
          {:else if row.type === "added"}
            <tr class="bg-emerald-950/20">
              <td class="w-4 select-none px-1.5 text-right text-emerald-400">+</td>
              <td class="whitespace-pre px-1.5 text-emerald-300">{row.line}</td>
            </tr>
          {:else}
            <tr class="text-muted-foreground">
              <td class="w-4 select-none px-1.5 text-right"> </td>
              <td class="whitespace-pre px-1.5">{row.line}</td>
            </tr>
          {/if}
        {/each}
      </tbody>
    </table>
  </div>

  <PermissionActionBar
    {taskId}
    toolName="file_edit"
    argPrefix={permission.file_path}
    {onResolved}
  />
</div>
