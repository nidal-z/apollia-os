<script lang="ts">
  /**
   * Back/forward controls for the Topbar. Tooltips show
   * the label of the route you would land on (e.g. "Back to Agents").
   */
  import { t } from "svelte-i18n";
  import { ChevronLeft, ChevronRight } from "lucide-svelte";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import { canGoBack, canGoForward, goBack, goForward } from "$lib/stores/navigation";
  import {
    previousLabelKey,
    nextLabelKey,
    backHistorySize,
  } from "$lib/navigation/historyStore";

  const backTooltip = $derived.by(() => {
    if (!$canGoBack) return $t("a11y.back_shortcut");
    if ($previousLabelKey) {
      const label = $t($previousLabelKey);
      if ($backHistorySize > 1) {
        return $t("topbar.back_to_with_count", { values: { label, count: $backHistorySize } });
      }
      return $t("topbar.back_to", { values: { label } });
    }
    return $t("a11y.back_shortcut");
  });

  const forwardTooltip = $derived.by(() => {
    if (!$canGoForward) return $t("a11y.forward_shortcut");
    if ($nextLabelKey) {
      return $t("topbar.forward_to", { values: { label: $t($nextLabelKey) } });
    }
    return $t("a11y.forward_shortcut");
  });
</script>

<div class="flex items-center gap-0.5">
  <Tooltip content={backTooltip} side="bottom">
    <button
      type="button"
      class="inline-flex h-9 w-9 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-30 disabled:pointer-events-none"
      onclick={goBack}
      disabled={!$canGoBack}
      aria-label={backTooltip}
      data-testid="topbar-back"
    >
      <ChevronLeft size={16} strokeWidth={1.75} />
    </button>
  </Tooltip>
  <Tooltip content={forwardTooltip} side="bottom">
    <button
      type="button"
      class="inline-flex h-9 w-9 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-30 disabled:pointer-events-none"
      onclick={goForward}
      disabled={!$canGoForward}
      aria-label={forwardTooltip}
      data-testid="topbar-forward"
    >
      <ChevronRight size={16} strokeWidth={1.75} />
    </button>
  </Tooltip>
</div>
