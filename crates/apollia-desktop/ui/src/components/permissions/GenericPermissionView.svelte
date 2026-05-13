<script lang="ts">
  import { t } from "svelte-i18n";
  import type { GenericPermission } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import JsonViewer from "../common/JsonViewer.svelte";
  import PermissionActionBar from "$lib/components/operator/permissions/PermissionActionBar.svelte";

  interface Props {
    taskId: string;
    permission: GenericPermission;
    projectPath?: string;
    onResolved: () => void;
  }

  let { taskId, permission, projectPath, onResolved }: Props = $props();

  const inputJson = $derived(JSON.stringify(permission.input, null, 2));
</script>

<div class="space-y-2" data-testid="generic-permission-view">
  <div class="flex flex-wrap items-center gap-2">
    <Badge variant="secondary">{$t("permissions.type_generic")}</Badge>
    <span class="text-[12px] font-medium">{permission.tool_name}</span>
  </div>

  <p class="text-[11px] font-medium text-muted-foreground">{$t("permissions.input_label")}</p>
  <JsonViewer json={inputJson} class="max-h-48 overflow-y-auto" />

  <PermissionActionBar
    {taskId}
    {projectPath}
    toolName={permission.tool_name}
    {onResolved}
  />
</div>
