<script lang="ts">
  import { t } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { Loader2, Wrench } from "lucide-svelte";
  import { Sheet } from "$lib/components/ui/sheet";
  import { Button } from "$lib/components/ui/button";
  import { formatRelativeTime } from "$lib/utils";
  import type { McpServerDetailView, McpServerStatusView } from "$lib/types";
  import ConnectionStatusIndicator from "./ConnectionStatusIndicator.svelte";
  import ApprovalLevelSelector from "./ApprovalLevelSelector.svelte";

  type ApprovalLevel = "auto" | "ask" | "readonly";

  interface Props {
    serverName: string;
    open: boolean;
    onclose: () => void;
    onDisconnect: () => void;
  }

  let { serverName, open, onclose, onDisconnect }: Props = $props();

  let detail = $state<McpServerDetailView | null>(null);
  let loading = $state(false);
  let fetchError = $state<string | null>(null);
  let approvalLevel = $state<ApprovalLevel>("ask");
  let approvalPending = $state(false);
  let approvalError = $state<string | null>(null);

  // Test action state
  type TestState = "idle" | "testing" | "ok" | "error";
  let testState = $state<TestState>("idle");
  let testStatus = $state<McpServerStatusView | null>(null);
  let testError = $state<string | null>(null);

  // Reconnect action state
  let reconnecting = $state(false);
  let reconnectError = $state<string | null>(null);

  // Disconnect action state
  let confirmDisconnect = $state(false);
  let disconnecting = $state(false);
  let disconnectError = $state<string | null>(null);

  function deriveApprovalLevel(requiresApproval: boolean): ApprovalLevel {
    return requiresApproval ? "ask" : "auto";
  }

  async function fetchDetail(): Promise<void> {
    loading = true;
    fetchError = null;
    try {
      detail = await invoke<McpServerDetailView>("get_mcp_server_detail", {
        name: serverName,
      });
      approvalLevel = deriveApprovalLevel(detail.config.requires_approval);
    } catch (err: unknown) {
      fetchError = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (open) {
      detail = null;
      testState = "idle";
      testStatus = null;
      testError = null;
      reconnectError = null;
      confirmDisconnect = false;
      disconnectError = null;
      approvalError = null;
      fetchDetail();
    }
  });

  async function handleApprovalChange(newLevel: ApprovalLevel): Promise<void> {
    if (!detail) return;
    approvalLevel = newLevel;
    approvalPending = true;
    approvalError = null;
    const requiresApproval = newLevel === "ask";
    try {
      await invoke("set_mcp_server_approval", {
        name: serverName,
        requiresApproval,
      });
      detail = {
        ...detail,
        config: { ...detail.config, requires_approval: requiresApproval },
        status: { ...detail.status, requires_approval: requiresApproval },
      };
    } catch (err: unknown) {
      approvalError = err instanceof Error ? err.message : String(err);
      // Revert on error
      approvalLevel = deriveApprovalLevel(detail.config.requires_approval);
    } finally {
      approvalPending = false;
    }
  }

  async function handleTest(): Promise<void> {
    testState = "testing";
    testError = null;
    testStatus = null;
    try {
      const refreshed = await invoke<McpServerDetailView>("get_mcp_server_detail", {
        name: serverName,
      });
      detail = refreshed;
      testStatus = refreshed.status;
      testState = refreshed.status.connected ? "ok" : "error";
      if (!refreshed.status.connected) {
        testError = refreshed.status.error ?? $t("integrations.manage.test_failed");
      }
    } catch (err: unknown) {
      testState = "error";
      testError = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleReconnect(): Promise<void> {
    reconnecting = true;
    reconnectError = null;
    testState = "idle";
    testStatus = null;
    try {
      const updated = await invoke<McpServerStatusView>("restart_mcp_server", {
        name: serverName,
      });
      if (detail) {
        detail = { ...detail, status: updated };
      }
      approvalLevel = deriveApprovalLevel(updated.requires_approval);
    } catch (err: unknown) {
      reconnectError = err instanceof Error ? err.message : String(err);
    } finally {
      reconnecting = false;
    }
  }

  async function handleDisconnect(): Promise<void> {
    if (!detail) return;
    disconnecting = true;
    disconnectError = null;
    try {
      for (const envKey of detail.config.env_keys) {
        await invoke("delete_mcp_secret", {
          serverName,
          envVar: envKey,
        });
      }
      await invoke("remove_mcp_server", { name: serverName });
      onDisconnect();
    } catch (err: unknown) {
      disconnectError = err instanceof Error ? err.message : String(err);
      disconnecting = false;
    }
  }

  const lastActivity = $derived(
    detail?.status.last_call_at
      ? formatRelativeTime(detail.status.last_call_at)
      : null,
  );
</script>

<Sheet {open} onclose={onclose}>
  <div class="flex flex-col gap-5 px-6 pt-6 pb-6 overflow-y-auto h-full">
    <!-- Header -->
    <div class="pr-6">
      <h2 class="text-base font-semibold text-foreground" data-testid="manage-sheet-title">
        {$t("integrations.manage.title", { values: { name: serverName } })}
      </h2>
    </div>

    {#if loading}
      <div class="flex items-center gap-2 text-sm text-muted-foreground" data-testid="manage-loading">
        <Loader2 size={14} class="animate-spin" />
        <span>{$t("common.loading")}</span>
      </div>
    {:else if fetchError}
      <p class="text-sm text-destructive" data-testid="manage-fetch-error">{fetchError}</p>
    {:else if detail}
      <!-- Connection status -->
      <section aria-labelledby="manage-status-heading">
        <h3
          id="manage-status-heading"
          class="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground"
        >
          {$t("integrations.manage.status_section")}
        </h3>
        <div class="glass-card glass-border rounded-lg px-4 py-3 flex flex-col gap-2">
          <ConnectionStatusIndicator
            connected={detail.status.connected}
            error={detail.status.error}
          />
          <div class="flex items-center gap-1.5 text-xs text-muted-foreground">
            <Wrench size={12} />
            <span data-testid="manage-tools-count">
              {$t("integrations.manage.tools_count", {
                values: { count: detail.status.tools_count },
              })}
            </span>
          </div>
          {#if detail.status.error}
            <p class="text-xs text-destructive" data-testid="manage-server-error">
              {detail.status.error}
            </p>
          {/if}
        </div>
      </section>

      <!-- Recent activity -->
      <section aria-labelledby="manage-activity-heading">
        <h3
          id="manage-activity-heading"
          class="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground"
        >
          {$t("integrations.manage.activity_section")}
        </h3>
        <div class="glass-card glass-border rounded-lg px-4 py-3">
          {#if lastActivity !== null}
            <p class="text-sm text-foreground" data-testid="manage-last-activity">
              {$t("integrations.manage.last_call", { values: { time: lastActivity } })}
            </p>
          {:else}
            <p class="text-sm text-muted-foreground" data-testid="manage-no-activity">
              {$t("integrations.manage.no_activity")}
            </p>
          {/if}
        </div>
      </section>

      <!-- Approval level -->
      <section aria-labelledby="manage-approval-heading">
        <h3
          id="manage-approval-heading"
          class="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground"
        >
          {$t("integrations.manage.approval_section")}
        </h3>
        <ApprovalLevelSelector
          level={approvalLevel}
          onchange={handleApprovalChange}
        />
        {#if approvalPending}
          <p class="mt-1.5 flex items-center gap-1 text-xs text-muted-foreground">
            <Loader2 size={11} class="animate-spin" />
            {$t("integrations.manage.approval_saving")}
          </p>
        {/if}
        {#if approvalError}
          <p class="mt-1.5 text-xs text-destructive" data-testid="manage-approval-error">
            {approvalError}
          </p>
        {/if}
      </section>

      <!-- Test result inline feedback -->
      {#if testState === "ok" && testStatus}
        <p
          class="text-sm text-emerald-600 dark:text-emerald-400"
          data-testid="manage-test-ok"
        >
          {$t("integrations.manage.test_ok", {
            values: { count: testStatus.tools_count },
          })}
        </p>
      {:else if testState === "error"}
        <p class="text-sm text-destructive" data-testid="manage-test-error">
          {testError ?? $t("integrations.manage.test_failed")}
        </p>
      {/if}

      {#if reconnectError}
        <p class="text-sm text-destructive" data-testid="manage-reconnect-error">
          {reconnectError}
        </p>
      {/if}

      <!-- Actions -->
      <section class="flex flex-col gap-2 pt-1">
        <div class="flex gap-2">
          <Button
            size="sm"
            variant="outline"
            class="flex-1"
            onclick={handleTest}
            disabled={testState === "testing"}
            data-testid="manage-test-btn"
          >
            {#if testState === "testing"}
              <Loader2 size={14} class="mr-1.5 animate-spin" />
              {$t("integrations.manage.test_testing")}
            {:else}
              {$t("integrations.manage.test_button")}
            {/if}
          </Button>
          <Button
            size="sm"
            variant="outline"
            class="flex-1"
            onclick={handleReconnect}
            disabled={reconnecting}
            data-testid="manage-reconnect-btn"
          >
            {#if reconnecting}
              <Loader2 size={14} class="mr-1.5 animate-spin" />
              {$t("integrations.manage.reconnecting")}
            {:else}
              {$t("integrations.manage.reconnect_button")}
            {/if}
          </Button>
        </div>

        <!-- Disconnect section -->
        {#if !confirmDisconnect}
          <Button
            size="sm"
            variant="destructive"
            onclick={() => { confirmDisconnect = true; }}
            data-testid="manage-disconnect-btn"
          >
            {$t("integrations.manage.disconnect_button")}
          </Button>
        {:else}
          <div
            class="rounded-lg border border-destructive/30 bg-destructive/5 px-4 py-3 flex flex-col gap-3"
            data-testid="manage-disconnect-confirm"
          >
            <p class="text-sm text-foreground">
              {$t("integrations.manage.disconnect_warning", {
                values: { name: serverName },
              })}
            </p>
            {#if disconnectError}
              <p class="text-xs text-destructive">{disconnectError}</p>
            {/if}
            <div class="flex gap-2">
              <Button
                size="sm"
                variant="outline"
                class="flex-1"
                onclick={() => { confirmDisconnect = false; disconnectError = null; }}
                disabled={disconnecting}
                data-testid="manage-disconnect-cancel"
              >
                {$t("integrations.manage.disconnect_cancel")}
              </Button>
              <Button
                size="sm"
                variant="destructive"
                class="flex-1"
                onclick={handleDisconnect}
                disabled={disconnecting}
                data-testid="manage-disconnect-confirm-btn"
              >
                {#if disconnecting}
                  <Loader2 size={14} class="mr-1.5 animate-spin" />
                  {$t("integrations.manage.disconnecting")}
                {:else}
                  {$t("integrations.manage.disconnect_confirm")}
                {/if}
              </Button>
            </div>
          </div>
        {/if}
      </section>
    {/if}
  </div>
</Sheet>
