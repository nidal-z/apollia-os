<script lang="ts">
  import { t } from "svelte-i18n";
  import type { FileWritePermission } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import PermissionActionBar from "./PermissionActionBar.svelte";

  const CONTENT_PREVIEW_LINES = 8;

  interface Props {
    taskId: string;
    permission: FileWritePermission;
    projectPath?: string;
    onResolved: () => void;
  }

  let { taskId, permission, projectPath, onResolved }: Props = $props();

  const contentPreview = $derived.by(() => {
    const lines = permission.content.split("\n");
    const preview = lines.slice(0, CONTENT_PREVIEW_LINES).join("\n");
    return lines.length > CONTENT_PREVIEW_LINES ? preview + "\n…" : preview;
  });
</script>

<div class="space-y-2" data-testid="file-write-permission-view">
  <div class="flex flex-wrap items-center gap-2">
    <Badge variant="info">{$t("permissions.type_file_write")}</Badge>
    {#if permission.mode === "create"}
      <Badge variant="success" data-testid="file-write-mode-badge">
        {$t("permissions.mode_create")}
      </Badge>
    {:else}
      <Badge variant="warning" data-testid="file-write-mode-badge">
        {$t("permissions.mode_overwrite")}
      </Badge>
    {/if}
  </div>

  <p class="text-[11px] text-muted-foreground">
    <span class="font-medium">{$t("permissions.file_label")}:</span>
    {permission.file_path}
  </p>

  <pre
    class="max-h-36 overflow-y-auto overflow-x-auto rounded glass-border glass-inset p-2.5 text-[12px] font-mono text-muted-foreground"
  >{contentPreview}</pre>

  <PermissionActionBar
    {taskId}
    {projectPath}
    toolName="file_write"
    argPrefix={permission.file_path}
    {onResolved}
  />
</div>
