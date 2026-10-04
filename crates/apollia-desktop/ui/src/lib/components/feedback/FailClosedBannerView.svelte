<script lang="ts">
  /**
   * FailClosedBannerView - what the fail-closed banner shows for one state of
   * governance, with no store and no IPC, so it renders under test.
   *
   * Full width under the top bar, a background tinted with the danger tone and
   * no border: the alert icon, "Sensitive actions blocked", why, and that
   * reading continues; at the right, one raised action.
   */
  import { t } from "svelte-i18n";
  import { fly } from "svelte/transition";
  import { AlertTriangle } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { rm } from "$lib/design/motion";
  import type { FailClosed } from "$lib/ipc/governance";
  import { failClosedBanner } from "$lib/governance/failClosedBanner";

  let {
    failClosed,
    retrying = false,
    onretry,
    onviewpolicy,
  }: {
    failClosed: FailClosed | null;
    retrying?: boolean;
    onretry?: () => void;
    onviewpolicy?: () => void;
  } = $props();

  const banner = $derived(failClosedBanner(failClosed));
</script>

{#if banner}
  <div
    class="flex items-center justify-between gap-4 bg-destructive/10 px-4 py-2.5"
    role="alert"
    aria-live="assertive"
    transition:fly={rm({ y: -16, duration: 280 })}
    data-testid="governance-fail-closed-banner"
    data-cause={failClosed?.cause}
  >
    <div class="flex min-w-0 flex-1 items-start gap-2">
      <AlertTriangle size={16} aria-hidden="true" class="mt-0.5 shrink-0 text-destructive" />
      <div class="min-w-0 flex-1">
        <p class="text-sm font-medium text-destructive" data-testid="governance-fail-closed-title">
          {$t("governance.fail_closed.title")}
        </p>
        <p class="text-xs text-foreground" data-testid="governance-fail-closed-reason">
          {$t(banner.reasonKey, {
            values: { reason: banner.policyReasonKey ? $t(banner.policyReasonKey) : "" },
          })}
        </p>
      </div>
    </div>
    <div class="shrink-0">
      {#if banner.action === "retry"}
        <Button
          variant="elevated"
          size="sm"
          loading={retrying}
          disabled={retrying}
          onclick={() => onretry?.()}
          data-testid="governance-fail-closed-retry"
        >
          {$t("governance.fail_closed.retry")}
        </Button>
      {:else}
        <Button
          variant="elevated"
          size="sm"
          onclick={() => onviewpolicy?.()}
          data-testid="governance-fail-closed-view-policy"
        >
          {$t("governance.fail_closed.view_policy")}
        </Button>
      {/if}
    </div>
  </div>
{/if}
