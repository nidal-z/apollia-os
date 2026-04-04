<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { BashPermission } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast/store";
  import { Terminal } from "lucide-svelte";

  interface Props {
    permission: BashPermission;
    onApprove: () => Promise<void>;
    onReject: (reason?: string) => Promise<void>;
  }

  let { permission, onApprove, onReject }: Props = $props();

  let busy = $state(false);
  let error = $state<string | null>(null);

  /** Extracts the first word of the command to use as arg_prefix for the prefix rule. */
  function extractCommandPrefix(cmd: string): string | null {
    const firstWord = cmd.trim().split(/\s+/)[0];
    return firstWord.length > 0 ? firstWord : null;
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
      const argPrefix = extractCommandPrefix(permission.command);
      await invoke("add_permission_prefix_rule", {
        toolName: "bash_executor",
        argPrefix,
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
  data-testid="bash-permission-view"
>
  <!-- Type badge header -->
  <div class="flex items-center gap-2 px-3 pt-2.5 pb-2 border-b border-border/30">
    <Terminal class="h-3.5 w-3.5 text-muted-foreground" />
    <Badge variant="secondary" class="text-[10px] uppercase tracking-wide">
      {$t("permissions.type_bash")}
    </Badge>
    <span class="ml-auto text-[10px] text-muted-foreground font-mono truncate max-w-[160px]">
      {permission.agent_id}
    </span>
  </div>

  <!-- Command preview -->
  <div class="px-3 py-2.5">
    <div
      class="rounded glass-inset glass-border p-2.5 font-mono text-[12px] text-foreground overflow-x-auto whitespace-pre"
      data-testid="bash-command"
    >
      {permission.command}
    </div>

    <!-- Working dir -->
    {#if permission.working_dir}
      <p class="mt-1.5 text-[10px] text-muted-foreground">
        <span class="font-medium">{$t("permissions.working_dir")}:</span>
        <span class="font-mono ml-1">{permission.working_dir}</span>
      </p>
    {/if}

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
        data-testid="bash-approve-btn"
      >
        {busy ? $t("permissions.approving") : $t("permissions.approve")}
      </Button>
      <Button
        variant="ghost"
        size="sm"
        class="h-7 px-3 text-[11px] text-destructive hover:bg-destructive/10 hover:text-destructive"
        disabled={busy}
        onclick={handleReject}
        data-testid="bash-reject-btn"
      >
        {$t("permissions.reject")}
      </Button>
      <Button
        variant="ghost"
        size="sm"
        class="h-7 px-3 text-[11px] text-primary"
        disabled={busy}
        onclick={handleAlwaysAllow}
        data-testid="bash-always-allow-btn"
      >
        {$t("permissions.always_allow")}
      </Button>
    </div>
  </div>
</div>
