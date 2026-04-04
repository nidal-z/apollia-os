<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { FilesystemPermission } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast/store";
  import { FolderCog } from "lucide-svelte";

  interface Props {
    permission: FilesystemPermission;
    onApprove: () => Promise<void>;
    onReject: (reason?: string) => Promise<void>;
  }

  let { permission, onApprove, onReject }: Props = $props();

  let busy = $state(false);
  let error = $state<string | null>(null);

  function operationLabel(op: FilesystemPermission["operation"]): string {
    switch (op) {
      case "delete": return $t("permissions.op_delete");
      case "move": return $t("permissions.op_move");
      case "mkdir": return $t("permissions.op_mkdir");
    }
  }

  function operationVariant(
    op: FilesystemPermission["operation"],
  ): "destructive" | "warning" | "info" {
    switch (op) {
      case "delete": return "destructive";
      case "move": return "warning";
      case "mkdir": return "info";
    }
  }

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
        toolName: `fs_${permission.operation}`,
        argPrefix: permission.path,
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
  data-testid="filesystem-permission-view"
>
  <!-- Type badge header -->
  <div class="flex items-center gap-2 px-3 pt-2.5 pb-2 border-b border-border/30">
    <FolderCog class="h-3.5 w-3.5 text-muted-foreground" />
    <Badge variant="secondary" class="text-[10px] uppercase tracking-wide">
      {$t("permissions.type_filesystem")}
    </Badge>
    <Badge variant={operationVariant(permission.operation)} class="text-[10px]" data-testid="fs-operation-badge">
      {operationLabel(permission.operation)}
    </Badge>
    <span class="ml-auto text-[10px] text-muted-foreground font-mono truncate max-w-[160px]">
      {permission.agent_id}
    </span>
  </div>

  <div class="px-3 py-2.5">
    <!-- Path info -->
    <div class="space-y-1.5">
      <p class="text-[10px] text-muted-foreground">
        <span class="font-medium">{$t("permissions.file_label")}:</span>
        <span class="font-mono ml-1 text-foreground break-all">{permission.path}</span>
      </p>
      {#if permission.target_path}
        <p class="text-[10px] text-muted-foreground">
          <span class="font-medium">{$t("permissions.target_label")}:</span>
          <span class="font-mono ml-1 text-foreground break-all">{permission.target_path}</span>
        </p>
      {/if}
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
        data-testid="fs-approve-btn"
      >
        {busy ? $t("permissions.approving") : $t("permissions.approve")}
      </Button>
      <Button
        variant="ghost"
        size="sm"
        class="h-7 px-3 text-[11px] text-destructive hover:bg-destructive/10 hover:text-destructive"
        disabled={busy}
        onclick={handleReject}
        data-testid="fs-reject-btn"
      >
        {$t("permissions.reject")}
      </Button>
      <Button
        variant="ghost"
        size="sm"
        class="h-7 px-3 text-[11px] text-primary"
        disabled={busy}
        onclick={handleAlwaysAllow}
        data-testid="fs-always-allow-btn"
      >
        {$t("permissions.always_allow")}
      </Button>
    </div>
  </div>
</div>
