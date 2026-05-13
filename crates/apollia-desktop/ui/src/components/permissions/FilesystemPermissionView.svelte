<script lang="ts">
  import { t } from "svelte-i18n";
  import type { FilesystemPermission } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import PermissionActionBar from "$lib/components/operator/permissions/PermissionActionBar.svelte";

  type BadgeVariant = "destructive" | "warning" | "info";

  interface Props {
    taskId: string;
    permission: FilesystemPermission;
    projectPath?: string;
    onResolved: () => void;
  }

  let { taskId, permission, projectPath, onResolved }: Props = $props();

  const opVariant = $derived<BadgeVariant>(
    permission.operation === "delete"
      ? "destructive"
      : permission.operation === "move"
        ? "warning"
        : "info",
  );

  const opLabel = $derived(
    permission.operation === "delete"
      ? $t("permissions.op_delete")
      : permission.operation === "move"
        ? $t("permissions.op_move")
        : $t("permissions.op_mkdir"),
  );
</script>

<div class="space-y-2" data-testid="filesystem-permission-view">
  <div class="flex flex-wrap items-center gap-2">
    <Badge variant="warning">{$t("permissions.type_filesystem")}</Badge>
    <Badge variant={opVariant} data-testid="filesystem-op-badge">{opLabel}</Badge>
  </div>

  <p class="text-[11px] text-muted-foreground">
    <span class="font-medium">{$t("permissions.file_label")}:</span>
    {permission.path}
  </p>

  {#if permission.target_path}
    <p class="text-[11px] text-muted-foreground">
      <span class="font-medium">{$t("permissions.target_label")}:</span>
      {permission.target_path}
    </p>
  {/if}

  <PermissionActionBar
    {taskId}
    {projectPath}
    toolName="filesystem"
    argPrefix={permission.path}
    {onResolved}
  />
</div>
