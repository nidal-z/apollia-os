<script lang="ts">
  /**
   * RuntimeStatusBanner - shared runtime-health banner.
   *
   * Rendered app-wide (see `Main.svelte`) and driven by the `runtimeHealth`
   * store: it mounts only while the runtime heartbeat is lost and surfaces the
   * current reconnect attempt plus a manual retry affordance. Unifies the
   * former chat `RuntimeDisconnectedBanner` and dashboard `OfflineBanner`.
   *
   * The underlying `Banner` primitive already respects reduced motion via
   * `$lib/design/motion`, so the enter/leave transition is safe.
   */
  import { t } from "svelte-i18n";
  import { WifiOff, RefreshCw } from "lucide-svelte";
  import { Banner } from "$lib/components/ui/banner";
  import { Button } from "$lib/components/ui/button";
  import { runtimeHealth, triggerReconnect } from "$lib/stores/runtimeHealth";
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
      <Button
        variant="outline"
        size="sm"
        class="h-auto border-destructive/40 py-0.5 text-xs font-medium text-destructive hover:bg-destructive/20 focus-visible:ring-destructive/60"
        onclick={triggerReconnect}
        data-testid="runtime-disconnected-retry"
      >
        {#snippet icon()}
          <RefreshCw size={11} aria-hidden="true" />
        {/snippet}
        {$t("chat.runtime_disconnected.retry")}
      </Button>
    {/snippet}
  </Banner>
{/if}
