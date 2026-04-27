<script lang="ts">
  /**
   * Topbar connection chip. Invisible when the runtime
   * is connected — the sidebar already shows a dot. Visible (pulsing
   * amber / static red) when the connection degrades, with a popover
   * that exposes a Retry action.
   */
  import { t } from "svelte-i18n";
  import { AlertTriangle, WifiOff, RefreshCw } from "lucide-svelte";
  import { Popover } from "$lib/components/ui/popover";
  import { Button } from "$lib/components/ui/button";
  import { connectionStatus, createSSEConnection } from "$lib/stores/sse";

  let open = $state(false);
  let retrying = $state(false);

  const isReconnecting = $derived($connectionStatus === "reconnecting" || $connectionStatus === "connecting");
  const isOffline = $derived($connectionStatus === "error");
  const visible = $derived(isReconnecting || isOffline);

  function retry() {
    if (retrying) return;
    retrying = true;
    try {
      createSSEConnection();
    } finally {
      setTimeout(() => (retrying = false), 500);
    }
  }
</script>

{#if visible}
  <Popover bind:open side="bottom" align="end" class="min-w-[240px] p-3">
    {#snippet trigger(triggerProps: Record<string, unknown>)}
      <button
        {...triggerProps}
        class="inline-flex h-9 items-center gap-1.5 rounded-full border px-2.5 text-xs font-medium transition-colors {isOffline
          ? 'border-destructive/40 bg-destructive/10 text-destructive'
          : 'border-warning/40 bg-warning/10 text-warning'}"
        aria-label={$t(isOffline ? 'topbar.connection.offline' : 'topbar.connection.reconnecting')}
        aria-live="polite"
        data-testid="connection-chip"
        data-status={$connectionStatus}
      >
        {#if isOffline}
          <WifiOff size={14} strokeWidth={1.75} />
          <span>{$t('topbar.connection.offline')}</span>
        {:else}
          <AlertTriangle size={14} strokeWidth={1.75} class="animate-pulse" />
          <span>{$t('topbar.connection.reconnecting')}</span>
        {/if}
      </button>
    {/snippet}
    {#snippet content()}
      <div class="flex flex-col gap-2" data-testid="connection-chip-popover">
        <p class="text-sm font-medium text-foreground">
          {$t(isOffline ? 'topbar.connection.offline_title' : 'topbar.connection.reconnecting_title')}
        </p>
        <p class="text-xs text-muted-foreground">
          {$t(isOffline ? 'topbar.connection.offline_detail' : 'topbar.connection.reconnecting_detail')}
        </p>
        <Button variant="secondary" size="sm" onclick={retry} disabled={retrying} data-testid="connection-chip-retry">
          <RefreshCw size={14} strokeWidth={1.75} class={retrying ? 'animate-spin' : ''} />
          <span>{$t('common.retry')}</span>
        </Button>
      </div>
    {/snippet}
  </Popover>
{/if}
