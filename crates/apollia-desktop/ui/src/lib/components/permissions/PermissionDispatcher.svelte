<script lang="ts">
  import type { ApollaPermission } from "$lib/types";
  import BashPermissionView from "./BashPermissionView.svelte";
  import FileEditPermissionView from "./FileEditPermissionView.svelte";
  import FileWritePermissionView from "./FileWritePermissionView.svelte";
  import FilesystemPermissionView from "./FilesystemPermissionView.svelte";
  import McpPermissionView from "./McpPermissionView.svelte";
  import GenericPermissionView from "./GenericPermissionView.svelte";

  interface Props {
    permission: ApollaPermission;
    onApprove: () => Promise<void>;
    onReject: (reason?: string) => Promise<void>;
  }

  let { permission, onApprove, onReject }: Props = $props();
</script>

{#if permission.permission_type === "bash"}
  <BashPermissionView
    permission={permission}
    {onApprove}
    {onReject}
  />
{:else if permission.permission_type === "file_edit"}
  <FileEditPermissionView
    permission={permission}
    {onApprove}
    {onReject}
  />
{:else if permission.permission_type === "file_write"}
  <FileWritePermissionView
    permission={permission}
    {onApprove}
    {onReject}
  />
{:else if permission.permission_type === "filesystem"}
  <FilesystemPermissionView
    permission={permission}
    {onApprove}
    {onReject}
  />
{:else if permission.permission_type === "mcp"}
  <McpPermissionView
    permission={permission}
    {onApprove}
    {onReject}
  />
{:else}
  <GenericPermissionView
    permission={{
      permission_type: "generic",
      request_id: permission.request_id,
      agent_id: permission.agent_id,
      tool_name: permission.permission_type,
      input: permission as unknown as Record<string, unknown>,
    }}
    {onApprove}
    {onReject}
  />
{/if}
