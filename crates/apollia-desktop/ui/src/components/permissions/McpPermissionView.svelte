<script lang="ts">
  import { t } from "svelte-i18n";
  import type { McpPermission } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import JsonViewer from "../common/JsonViewer.svelte";
  import PermissionActionBar from "$lib/components/operator/permissions/PermissionActionBar.svelte";

  interface Props {
    taskId: string;
    permission: McpPermission;
    projectPath?: string;
    onResolved: () => void;
  }

  let { taskId, permission, projectPath, onResolved }: Props = $props();

  const argsJson = $derived(JSON.stringify(permission.arguments, null, 2));
</script>

<div class="space-y-2" data-testid="mcp-permission-view">
  <Badge variant="info">{$t("permissions.type_mcp")}</Badge>

  <div class="grid grid-cols-[auto_1fr] gap-x-2 gap-y-1 text-[11px]">
    <span class="font-medium text-muted-foreground">{$t("permissions.server_label")}:</span>
    <span>{permission.server_name}</span>
    <span class="font-medium text-muted-foreground">{$t("permissions.tool_label")}:</span>
    <span>{permission.tool_name}</span>
  </div>

  <p class="text-[11px] font-medium text-muted-foreground">{$t("permissions.arguments_label")}</p>
  <JsonViewer json={argsJson} class="max-h-48 overflow-y-auto" />

  <PermissionActionBar
    {taskId}
    {projectPath}
    toolName={permission.tool_name}
    argPrefix={permission.server_name}
    {onResolved}
  />
</div>
