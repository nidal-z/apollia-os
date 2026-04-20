<script lang="ts">
  /**
   * Offline banner rendered when the initial onboarding asset fetch
   * times out (8 s) or fails. Local i18n + SVGs remain functional, so
   * the flow never blocks — the banner just surfaces the degraded
   * network condition and offers a manual retry (US-SP42-081).
   */
  import { t } from "svelte-i18n";
  import { WifiOff, RefreshCw } from "lucide-svelte";

  interface Props {
    onRetry: () => void;
    retrying?: boolean;
  }

  let { onRetry, retrying = false }: Props = $props();
</script>

<div
  role="status"
  aria-live="polite"
  class="flex items-start gap-3 rounded-xl border border-warning/40 bg-warning/10 px-4 py-3 text-sm text-foreground"
  data-testid="onboarding-offline-banner"
>
  <WifiOff size={18} strokeWidth={1.75} class="mt-0.5 shrink-0 text-warning" />
  <div class="flex-1 min-w-0">
    <p class="font-medium">{$t("onboarding_welcome.offline_title")}</p>
    <p class="text-xs text-muted-foreground">
      {$t("onboarding_welcome.offline_body")}
    </p>
  </div>
  <button
    type="button"
    onclick={onRetry}
    disabled={retrying}
    class="inline-flex h-9 shrink-0 items-center gap-1.5 rounded-md border border-border bg-background px-3 text-xs font-medium text-foreground transition-colors hover:bg-muted disabled:opacity-50"
    data-testid="onboarding-offline-retry"
  >
    <RefreshCw size={14} strokeWidth={1.75} class={retrying ? "animate-spin" : ""} />
    {$t("onboarding_welcome.offline_retry")}
  </button>
</div>
