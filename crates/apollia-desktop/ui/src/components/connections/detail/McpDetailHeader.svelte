<script lang="ts">
  /**
   * McpDetailHeader - identity header for an MCP server detail pane.
   *
   * Leading tokenised tile (focal glow), title + status badge + sync label, and
   * the Test / Reconnect actions. Presentation only; the parent owns the flows.
   */
  import { t } from "svelte-i18n";
  import { RefreshCw, Link as LinkIcon } from "lucide-svelte";
  import { DetailHeader } from "$lib/components/operator";
  import type { ConnectionStatus } from "$lib/connections/status";
  import { Button } from "$lib/components/ui/button";
  import { Spinner } from "$lib/components/ui/progress";
  import ConnectorTile from "../shared/ConnectorTile.svelte";
  import StatusBadge from "../shared/StatusBadge.svelte";
  import { accentTokenFor } from "$lib/connections/accent";
  import type { I18nDescriptor } from "$lib/connections/status";

  interface Props {
    label: string;
    meta: string;
    status: ConnectionStatus;
    sync: I18nDescriptor | null;
    testing: boolean;
    reconnecting: boolean;
    ontest: () => void;
    onreconnect: () => void;
  }

  let { label, meta, status, sync, testing, reconnecting, ontest, onreconnect }: Props = $props();
</script>

<DetailHeader title={label} titleTestid="mcp-detail-title" {meta}>
  {#snippet leading()}
    <ConnectorTile size="lg" accent={accentTokenFor(label)} glow>
      {#snippet icon()}<LinkIcon size={18} />{/snippet}
    </ConnectorTile>
  {/snippet}
  {#snippet badges()}
    <StatusBadge {status} />
    {#if sync}
      <span class="text-caption text-muted-foreground">
        {$t(sync.key, sync.values ? { values: sync.values } : undefined)}
      </span>
    {/if}
  {/snippet}
  {#snippet actions()}
    <Button variant="outline" size="sm" onclick={ontest} disabled={testing} data-testid="mcp-test-btn">
      {#snippet icon()}
        {#if testing}<Spinner size={12} />{:else}<RefreshCw size={12} />{/if}
      {/snippet}
      {testing ? $t("connections.testing") : $t("connections.test")}
    </Button>
    <Button
      variant="primary-solid"
      size="sm"
      onclick={onreconnect}
      disabled={reconnecting}
      data-testid="mcp-reconnect-btn"
    >
      {#snippet icon()}
        {#if reconnecting}<Spinner size={12} />{:else}<RefreshCw size={12} />{/if}
      {/snippet}
      {$t("connections.reconnect")}
    </Button>
  {/snippet}
</DetailHeader>
