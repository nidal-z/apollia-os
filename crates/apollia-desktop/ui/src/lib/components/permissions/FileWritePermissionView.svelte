<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { FileWritePermission } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast/store";
  import { FileOutput } from "lucide-svelte";

  interface Props {
    permission: FileWritePermission;
    onApprove: () => Promise<void>;
    onReject: (reason?: string) => Promise<void>;
  }

  let { permission, onApprove, onReject }: Props = $props();

  let busy = $state(false);
  let error = $state<string | null>(null);

  const MAX_PREVIEW_LINES = 60;

  const contentPreview = $derived(() => {
    const lines = permission.content.split("\n");
    if (lines.length <= MAX_PREVIEW_LINES) return permission.content;
    return (
      lines.slice(0, MAX_PREVIEW_LINES).join("\n") +
      `\n… (${lines.length - MAX_PREVIEW_LINES} more lines)`
    );
  });

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
        toolName: "file_write",
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
  data-testid="file-write-permission-view"
>
  <!-- Type badge header -->
  <div class="flex items-center gap-2 px-3 pt-2.5 pb-2 border-b border-border/30">
    <FileOutput class="h-3.5 w-3.5 text-muted-foreground" />
    <Badge variant="secondary" class="text-[10px] uppercase tracking-wide">
      {$t("permissions.type_file_write")}
    </Badge>
    <!-- Mode badge: green for create, orange for overwrite -->
    {#if permission.mode === "create"}
      <Badge variant="success" class="text-[10px]" data-testid="file-write-mode-badge">
        {$t("permissions.mode_create")}
      </Badge>
    {:else}
      <Badge variant="warning" class="text-[10px]" data-testid="file-write-mode-badge">
        {$t("permissions.mode_overwrite")}
      </Badge>
    {/if}
    <span class="ml-auto text-[10px] text-muted-foreground font-mono truncate max-w-[180px]">
      {permission.agent_id}
    </span>
  </div>

  <div class="px-3 py-2.5">
    <!-- File path -->
    <p class="text-[10px] text-muted-foreground mb-1.5">
      <span class="font-medium">{$t("permissions.file_label")}:</span>
      <span class="font-mono ml-1 text-foreground">{permission.file_path}</span>
    </p>

    <!-- Content preview -->
    <div
      class="rounded glass-inset glass-border p-2 font-mono text-[11px] leading-5 overflow-auto max-h-[240px] whitespace-pre text-foreground"
      data-testid="file-write-content"
    >
      {contentPreview()}
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
        data-testid="file-write-approve-btn"
      >
        {busy ? $t("permissions.approving") : $t("permissions.approve")}
      </Button>
      <Button
        variant="ghost"
        size="sm"
        class="h-7 px-3 text-[11px] text-destructive hover:bg-destructive/10 hover:text-destructive"
        disabled={busy}
        onclick={handleReject}
        data-testid="file-write-reject-btn"
      >
        {$t("permissions.reject")}
      </Button>
      <Button
        variant="ghost"
        size="sm"
        class="h-7 px-3 text-[11px] text-primary"
        disabled={busy}
        onclick={handleAlwaysAllow}
        data-testid="file-write-always-allow-btn"
      >
        {$t("permissions.always_allow")}
      </Button>
    </div>
  </div>
</div>
