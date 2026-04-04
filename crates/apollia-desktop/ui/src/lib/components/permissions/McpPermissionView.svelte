<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { McpPermission } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast/store";
  import { Plug } from "lucide-svelte";

  interface Props {
    permission: McpPermission;
    onApprove: () => Promise<void>;
    onReject: (reason?: string) => Promise<void>;
  }

  let { permission, onApprove, onReject }: Props = $props();

  let busy = $state(false);
  let error = $state<string | null>(null);

  const argsJson = $derived(JSON.stringify(permission.arguments, null, 2));

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
      // MCP tool invocations are identified by server_name + tool_name — no arg prefix.
      await invoke("add_permission_prefix_rule", {
        toolName: `${permission.server_name}::${permission.tool_name}`,
        argPrefix: null,
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
  data-testid="mcp-permission-view"
>
  <!-- Type badge header -->
  <div class="flex items-center gap-2 px-3 pt-2.5 pb-2 border-b border-border/30">
    <Plug class="h-3.5 w-3.5 text-muted-foreground" />
    <Badge variant="secondary" class="text-[10px] uppercase tracking-wide">
      {$t("permissions.type_mcp")}
    </Badge>
    <span class="ml-auto text-[10px] text-muted-foreground font-mono truncate max-w-[160px]">
      {permission.agent_id}
    </span>
  </div>

  <div class="px-3 py-2.5">
    <!-- Server + tool info -->
    <div class="flex flex-wrap gap-x-4 gap-y-1 mb-2.5">
      <p class="text-[10px] text-muted-foreground">
        <span class="font-medium">{$t("permissions.server_label")}:</span>
        <span class="ml-1 text-foreground font-mono" data-testid="mcp-server-name">
          {permission.server_name}
        </span>
      </p>
      <p class="text-[10px] text-muted-foreground">
        <span class="font-medium">{$t("permissions.tool_label")}:</span>
        <span class="ml-1 text-foreground font-mono" data-testid="mcp-tool-name">
          {permission.tool_name}
        </span>
      </p>
    </div>

    <!-- Arguments JSON -->
    <p class="text-[10px] text-muted-foreground mb-1">
      {$t("permissions.arguments_label")}
    </p>
    <div
      class="rounded glass-inset glass-border p-2 font-mono text-[11px] leading-5 overflow-auto max-h-[200px] whitespace-pre text-foreground"
      data-testid="mcp-arguments"
    >
      {argsJson}
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
        data-testid="mcp-approve-btn"
      >
        {busy ? $t("permissions.approving") : $t("permissions.approve")}
      </Button>
      <Button
        variant="ghost"
        size="sm"
        class="h-7 px-3 text-[11px] text-destructive hover:bg-destructive/10 hover:text-destructive"
        disabled={busy}
        onclick={handleReject}
        data-testid="mcp-reject-btn"
      >
        {$t("permissions.reject")}
      </Button>
      <Button
        variant="ghost"
        size="sm"
        class="h-7 px-3 text-[11px] text-primary"
        disabled={busy}
        onclick={handleAlwaysAllow}
        data-testid="mcp-always-allow-btn"
      >
        {$t("permissions.always_allow")}
      </Button>
    </div>
  </div>
</div>
