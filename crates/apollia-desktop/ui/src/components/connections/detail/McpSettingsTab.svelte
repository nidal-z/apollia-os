<script lang="ts">
  /**
   * McpSettingsTab - launch config, approval level, and the disconnect flow.
   *
   * Reuses `McpServerSettingsEditor` (generic launch config) and
   * `ApprovalLevelSelector`. The destructive disconnect is confirm-gated.
   */
  import { t } from "svelte-i18n";
  import { Trash2 } from "lucide-svelte";
  import { Card } from "$lib/components/ui/card";
  import { Button } from "$lib/components/ui/button";
  import { Spinner } from "$lib/components/ui/progress";
  import ApprovalLevelSelector from "$lib/components/operator/approval/ApprovalLevelSelector.svelte";
  import McpServerSettingsEditor from "../../integrations/McpServerSettingsEditor.svelte";
  import type { ApprovalLevel } from "../shared/types";

  interface Props {
    serverName: string;
    approvalLevel: ApprovalLevel;
    approvalPending: boolean;
    approvalError: string | null;
    confirmDisconnect: boolean;
    disconnecting: boolean;
    disconnectError: string | null;
    onapprovalchange: (level: ApprovalLevel) => void;
    onsaved: () => void;
    onrequestdisconnect: () => void;
    oncanceldisconnect: () => void;
    onconfirmdisconnect: () => void;
  }

  let {
    serverName,
    approvalLevel,
    approvalPending,
    approvalError,
    confirmDisconnect,
    disconnecting,
    disconnectError,
    onapprovalchange,
    onsaved,
    onrequestdisconnect,
    oncanceldisconnect,
    onconfirmdisconnect,
  }: Props = $props();
</script>

<div class="max-w-3xl space-y-4">
  <McpServerSettingsEditor {serverName} onSaved={onsaved} />

  <Card class="px-4 py-3.5">
    <div class="mb-2 text-overline text-muted-foreground/60">
      {$t("integrations.manage.approval_section")}
    </div>
    <ApprovalLevelSelector level={approvalLevel} onchange={onapprovalchange} />
    {#if approvalPending}
      <p class="mt-1.5 flex items-center gap-1 text-caption text-muted-foreground">
        <Spinner size={11} />
        {$t("integrations.manage.approval_saving")}
      </p>
    {/if}
    {#if approvalError}
      <p class="mt-1.5 text-caption text-destructive" data-testid="mcp-approval-error">{approvalError}</p>
    {/if}
  </Card>

  {#if !confirmDisconnect}
    <Button variant="destructive" size="sm" onclick={onrequestdisconnect} data-testid="mcp-disconnect-btn">
      {#snippet icon()}<Trash2 size={12} />{/snippet}
      {$t("integrations.manage.disconnect_button")}
    </Button>
  {:else}
    <Card class="space-y-3 border-destructive/30 bg-destructive/5 px-4 py-3.5" data-testid="mcp-disconnect-confirm">
      <p class="text-body-sm text-foreground">
        {$t("integrations.manage.disconnect_warning", { values: { name: serverName } })}
      </p>
      {#if disconnectError}
        <p class="text-caption text-destructive">{disconnectError}</p>
      {/if}
      <div class="flex gap-2">
        <Button variant="outline" size="sm" onclick={oncanceldisconnect} disabled={disconnecting}>
          {$t("common.cancel")}
        </Button>
        <Button
          variant="destructive"
          size="sm"
          onclick={onconfirmdisconnect}
          disabled={disconnecting}
          data-testid="mcp-disconnect-confirm-btn"
        >
          {#if disconnecting}<Spinner size={12} class="mr-1.5" />{$t("connections.disconnecting")}{:else}{$t(
              "connections.confirm_disconnect",
            )}{/if}
        </Button>
      </div>
    </Card>
  {/if}
</div>
