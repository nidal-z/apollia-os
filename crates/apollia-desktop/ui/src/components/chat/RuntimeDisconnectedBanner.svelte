<script lang="ts">
  /**
   * RuntimeDisconnectedBanner — persistent top-of-chat banner shown
   * whenever the runtime heartbeat times out (US-SP42-034).
   *
   * Displays current reconnect attempt + a manual retry affordance.
   * Wired to `runtimeHealth` store; mounts/dismounts itself based on
   * the live status.
   */
  import { t } from "svelte-i18n";
  import { WifiOff, RefreshCw } from "lucide-svelte";
  import {
    runtimeHealth,
    triggerReconnect,
  } from "$lib/stores/runtimeHealth";
</script>

{#if $runtimeHealth.status !== "connected"}
  <div
    class="flex items-center justify-between gap-3 border-b border-destructive/30
      bg-destructive/10 px-4 py-2 text-[12px] text-destructive"
    role="alert"
    aria-live="assertive"
    data-testid="runtime-disconnected-banner"
  >
    <div class="flex items-center gap-2">
      <WifiOff size={14} aria-hidden="true" />
      <span>
        {$runtimeHealth.status === "reconnecting"
          ? $t("chat.runtime_disconnected.reconnecting", {
              values: { attempt: $runtimeHealth.attempt },
            })
          : $t("chat.runtime_disconnected.lost")}
      </span>
    </div>
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
  </div>
{/if}
