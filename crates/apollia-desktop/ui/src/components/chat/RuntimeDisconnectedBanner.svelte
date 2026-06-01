<script lang="ts">
  /**
   * RuntimeDisconnectedBanner - persistent top-of-chat banner shown
   * whenever the runtime heartbeat times out.
   *
   * Displays current reconnect attempt + a manual retry affordance.
   * Wired to `runtimeHealth` store; mounts/dismounts itself based on
   * the live status.
   */
  import { t } from "svelte-i18n";
  import { WifiOff, RefreshCw } from "lucide-svelte";
  import { Banner } from "$lib/components/ui/banner";
  import {
    runtimeHealth,
    triggerReconnect,
  } from "$lib/stores/runtimeHealth";
</script>

{#if $runtimeHealth.status !== "connected"}
  <Banner
    variant="destructive"
    icon={WifiOff}
    data-testid="runtime-disconnected-banner"
  >
    {$runtimeHealth.status === "reconnecting"
      ? $t("chat.runtime_disconnected.reconnecting", {
          values: { attempt: $runtimeHealth.attempt },
        })
      : $t("chat.runtime_disconnected.lost")}

    {#snippet trailing()}
      <button
        type="button"
        class="inline-flex items-center gap-1 rounded border border-destructive/40 px-2 py-0.5
          text-[11px] font-medium transition-colors
          hover:bg-destructive/20 focus-visible:outline-none
          focus-visible:ring-2 focus-visible:ring-destructive/60"
        onclick={triggerReconnect}
        data-testid="runtime-disconnected-retry"
      >
        <RefreshCw size={11} aria-hidden="true" />
        {$t("chat.runtime_disconnected.retry")}
      </button>
    {/snippet}
  </Banner>
{/if}
