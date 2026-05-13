<script lang="ts">
  import { t } from "svelte-i18n";
  import hljs from "highlight.js/lib/core";
  import bash from "highlight.js/lib/languages/bash";
  import type { BashPermission } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import PermissionActionBar from "$lib/components/operator/permissions/PermissionActionBar.svelte";

  if (!hljs.getLanguage("bash")) {
    hljs.registerLanguage("bash", bash);
  }

  interface Props {
    taskId: string;
    permission: BashPermission;
    projectPath?: string;
    onResolved: () => void;
  }

  let { taskId, permission, projectPath, onResolved }: Props = $props();

  const highlighted = $derived(
    hljs.highlight(permission.command, { language: "bash" }).value,
  );

  const argPrefix = $derived(
    permission.command.trim().split(/\s+/)[0] || undefined,
  );
</script>

<div class="space-y-2" data-testid="bash-permission-view">
  <Badge variant="warning">{$t("permissions.type_bash")}</Badge>

  <pre
    class="hljs overflow-x-auto rounded glass-border glass-inset p-2.5 text-[13px] font-mono leading-relaxed"
  ><code>{@html highlighted}</code></pre>

  {#if permission.working_dir}
    <p class="text-[11px] text-muted-foreground">
      <span class="font-medium">{$t("permissions.working_dir")}:</span>
      {permission.working_dir}
    </p>
  {/if}

  <PermissionActionBar
    {taskId}
    {projectPath}
    toolName="bash"
    {argPrefix}
    {onResolved}
  />
</div>
