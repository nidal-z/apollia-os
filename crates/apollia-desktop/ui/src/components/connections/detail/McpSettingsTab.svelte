<script lang="ts">
  /**
   * McpSettingsTab - launch config and the disconnect flow.
   *
   * Reuses `McpServerSettingsEditor` (generic launch config). The destructive
   * disconnect is confirm-gated.
   */
  import { t } from "svelte-i18n";
  import { Trash2 } from "lucide-svelte";
  import { Card } from "$lib/components/ui/card";
  import { Button } from "$lib/components/ui/button";
  import { Spinner } from "$lib/components/ui/progress";
  import McpServerSettingsEditor from "../../integrations/McpServerSettingsEditor.svelte";

  interface Props {
    serverName: string;
    confirmDisconnect: boolean;
    disconnecting: boolean;
    disconnectError: string | null;
    onsaved: () => void;
    onrequestdisconnect: () => void;
    oncanceldisconnect: () => void;
    onconfirmdisconnect: () => void;
  }

  let {
    serverName,
    confirmDisconnect,
    disconnecting,
    disconnectError,
    onsaved,
    onrequestdisconnect,
    oncanceldisconnect,
    onconfirmdisconnect,
  }: Props = $props();
</script>

<div class="max-w-3xl space-y-4">
  <McpServerSettingsEditor {serverName} onSaved={onsaved} />

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
