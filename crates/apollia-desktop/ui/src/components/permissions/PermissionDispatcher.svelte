<script lang="ts">
  import type {
    ApollaPermission,
    BashPermission,
    FileEditPermission,
    FileWritePermission,
    FilesystemPermission,
    McpPermission,
    GenericPermission,
  } from "$lib/types";
  import BashPermissionView from "./BashPermissionView.svelte";
  import FileEditPermissionView from "./FileEditPermissionView.svelte";
  import FileWritePermissionView from "./FileWritePermissionView.svelte";
  import FilesystemPermissionView from "./FilesystemPermissionView.svelte";
  import McpPermissionView from "./McpPermissionView.svelte";
  import GenericPermissionView from "./GenericPermissionView.svelte";

  interface Props {
    /** Identifiant de la tâche suspendue à résoudre. */
    taskId: string;
    /** Demande de permission discriminée par `permission_type`. */
    permission: ApollaPermission;
    /** Appelé une fois la décision confirmée par le runtime. */
    onResolved: () => void;
  }

  let { taskId, permission, onResolved }: Props = $props();
</script>

<div data-testid="permission-dispatcher">
  {#if permission.permission_type === "bash"}
    <BashPermissionView
      {taskId}
      permission={permission as BashPermission}
      {onResolved}
    />
  {:else if permission.permission_type === "file_edit"}
    <FileEditPermissionView
      {taskId}
      permission={permission as FileEditPermission}
      {onResolved}
    />
  {:else if permission.permission_type === "file_write"}
    <FileWritePermissionView
      {taskId}
      permission={permission as FileWritePermission}
      {onResolved}
    />
  {:else if permission.permission_type === "filesystem"}
    <FilesystemPermissionView
      {taskId}
      permission={permission as FilesystemPermission}
      {onResolved}
    />
  {:else if permission.permission_type === "mcp"}
    <McpPermissionView
      {taskId}
      permission={permission as McpPermission}
      {onResolved}
    />
  {:else}
    <GenericPermissionView
      {taskId}
      permission={permission as GenericPermission}
      {onResolved}
    />
  {/if}
</div>
