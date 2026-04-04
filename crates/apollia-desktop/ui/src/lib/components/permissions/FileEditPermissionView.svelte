<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { FileEditPermission } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast/store";
  import { FilePenLine } from "lucide-svelte";

  interface Props {
    permission: FileEditPermission;
    onApprove: () => Promise<void>;
    onReject: (reason?: string) => Promise<void>;
  }

  let { permission, onApprove, onReject }: Props = $props();

  let busy = $state(false);
  let error = $state<string | null>(null);

  type DiffLine = { type: "equal" | "added" | "removed"; text: string };

  /** Computes a line-level diff between two texts using LCS. */
  function computeDiff(oldText: string, newText: string): DiffLine[] {
    const oldLines = oldText.split("\n");
    const newLines = newText.split("\n");
    const m = oldLines.length;
    const n = newLines.length;

    // Build LCS DP table.
    const dp: number[][] = Array.from({ length: m + 1 }, () =>
      new Array<number>(n + 1).fill(0),
    );
    for (let i = 1; i <= m; i++) {
      for (let j = 1; j <= n; j++) {
        if (oldLines[i - 1] === newLines[j - 1]) {
          dp[i][j] = dp[i - 1][j - 1] + 1;
        } else {
          dp[i][j] = Math.max(dp[i - 1][j], dp[i][j - 1]);
        }
      }
    }

    // Backtrack to build diff result.
    const result: DiffLine[] = [];
    let i = m;
    let j = n;
    while (i > 0 || j > 0) {
      if (
        i > 0 &&
        j > 0 &&
        oldLines[i - 1] === newLines[j - 1]
      ) {
        result.unshift({ type: "equal", text: oldLines[i - 1] });
        i--;
        j--;
      } else if (
        j > 0 &&
        (i === 0 || dp[i][j - 1] >= dp[i - 1][j])
      ) {
        result.unshift({ type: "added", text: newLines[j - 1] });
        j--;
      } else {
        result.unshift({ type: "removed", text: oldLines[i - 1] });
        i--;
      }
    }

    return result;
  }

  const diffLines = $derived(
    computeDiff(permission.old_content, permission.new_content),
  );

  async function handleApprove(): Promise<void> {
    busy = true;
    error = null;
    try {
      await onApprove();
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function handleReject(): Promise<void> {
    busy = true;
    error = null;
    try {
      await onReject();
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function handleAlwaysAllow(): Promise<void> {
    busy = true;
    error = null;
    try {
      await invoke("add_permission_prefix_rule", {
        toolName: "file_editor",
        argPrefix: permission.file_path,
        action: "allow",
      });
      addToast($t("permissions.always_allow_success"), "success");
      await onApprove();
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      error = msg;
      addToast($t("permissions.always_allow_error"), "error");
    } finally {
      busy = false;
    }
  }
</script>

<div
  class="rounded-lg glass-surface glass-border-subtle overflow-hidden"
  data-testid="file-edit-permission-view"
>
  <!-- Type badge header -->
  <div class="flex items-center gap-2 px-3 pt-2.5 pb-2 border-b border-border/30">
    <FilePenLine class="h-3.5 w-3.5 text-muted-foreground" />
    <Badge variant="secondary" class="text-[10px] uppercase tracking-wide">
      {$t("permissions.type_file_edit")}
    </Badge>
    <span class="ml-auto text-[10px] text-muted-foreground font-mono truncate max-w-[200px]">
      {permission.file_path}
    </span>
  </div>

  <!-- Diff preview -->
  <div class="px-3 py-2.5">
    <p class="text-[10px] text-muted-foreground mb-1.5">
      <span class="font-medium">{$t("permissions.file_label")}:</span>
      <span class="font-mono ml-1 text-foreground">{permission.file_path}</span>
    </p>

    <div
      class="rounded glass-inset glass-border overflow-auto max-h-[320px] font-mono text-[11px] leading-5"
      data-testid="file-edit-diff"
    >
      {#each diffLines as line}
        {#if line.type === "added"}
          <div class="px-2 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300">
            <span class="select-none mr-2 opacity-60">+</span>{line.text}
          </div>
        {:else if line.type === "removed"}
          <div class="px-2 bg-red-500/10 text-red-700 dark:text-red-300">
            <span class="select-none mr-2 opacity-60">−</span>{line.text}
          </div>
        {:else}
          <div class="px-2 text-muted-foreground">
            <span class="select-none mr-2 opacity-40"> </span>{line.text}
          </div>
        {/if}
      {/each}
    </div>

    {#if error}
      <p class="mt-1.5 text-[10px] text-destructive">{error}</p>
    {/if}

    <!-- Action buttons -->
    <div class="mt-3 flex gap-2">
      <Button
        variant="success"
        size="sm"
        class="h-7 px-3 text-[11px]"
        disabled={busy}
        onclick={handleApprove}
        data-testid="file-edit-approve-btn"
      >
        {busy ? $t("permissions.approving") : $t("permissions.approve")}
      </Button>
      <Button
        variant="ghost"
        size="sm"
        class="h-7 px-3 text-[11px] text-destructive hover:bg-destructive/10 hover:text-destructive"
        disabled={busy}
        onclick={handleReject}
        data-testid="file-edit-reject-btn"
      >
        {$t("permissions.reject")}
      </Button>
      <Button
        variant="ghost"
        size="sm"
        class="h-7 px-3 text-[11px] text-primary"
        disabled={busy}
        onclick={handleAlwaysAllow}
        data-testid="file-edit-always-allow-btn"
      >
        {$t("permissions.always_allow")}
      </Button>
    </div>
  </div>
</div>
