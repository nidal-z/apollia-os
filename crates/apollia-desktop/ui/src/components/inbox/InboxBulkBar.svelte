<script lang="ts">
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    count: number;
    highRiskCount: number;
    submitting: boolean;
    onapprove: () => void;
    onreject: () => void;
    oncancel: () => void;
  }

  let { count, highRiskCount, submitting, onapprove, onreject, oncancel }: Props = $props();
</script>

{#if count > 0}
  <div
    class="sticky bottom-0 left-0 right-0 z-10 flex items-center justify-between gap-3 border-t border-border bg-card/95 px-4 py-3 shadow-[0_-6px_24px_rgba(0,0,0,0.08)] backdrop-blur"
    role="region"
    aria-live="polite"
    aria-label={$t("inbox.bulk_bar_aria")}
    data-testid="inbox-bulk-bar"
  >
    <div class="flex items-center gap-2 text-[12px]">
      <span class="font-medium">{$t("inbox.bulk_selected", { values: { count } })}</span>
      {#if highRiskCount > 0}
        <span class="text-destructive">
          {$t("inbox.bulk_high_risk_warning", { values: { count: highRiskCount } })}
        </span>
      {/if}
    </div>
    <div class="flex items-center gap-2">
      <Button variant="outline" size="sm" onclick={oncancel} disabled={submitting}>
        {$t("common.cancel")}
      </Button>
      <Button
        variant="destructive"
        size="sm"
        onclick={onreject}
        disabled={submitting}
        data-testid="inbox-bulk-reject"
      >
        {$t("inbox.bulk_reject", { values: { count } })}
      </Button>
      <Button
        variant="success"
        size="sm"
        onclick={onapprove}
        disabled={submitting}
        data-testid="inbox-bulk-approve"
      >
        {$t("inbox.bulk_approve", { values: { count } })}
      </Button>
    </div>
  </div>
{/if}
