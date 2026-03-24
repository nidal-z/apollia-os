<!--
  Watches the extractedInsights store and shows a toast when new insights
  arrive. Opens the InsightsFeedback Sheet panel on user action.
  Mounted once in App.svelte so it works across all routes.
-->
<script lang="ts">
  import { get } from "svelte/store";
  import { t } from "svelte-i18n";
  import { uiMode } from "$lib/stores/mode";
  import { extractedInsights } from "$lib/stores/chat";
  import { addToast } from "$lib/components/ui/toast/store";
  import InsightsFeedback from "./InsightsFeedback.svelte";

  let feedbackOpen = $state(false);
  let previousLength = 0;

  const EXTRACTION_TOAST_DURATION_MS = 10_000;

  function openPanel(): void {
    feedbackOpen = true;
  }

  function closePanel(): void {
    feedbackOpen = false;
  }

  extractedInsights.subscribe((insights) => {
    if (insights.length > 0 && insights.length !== previousLength) {
      const mode = get(uiMode);
      const toastKey = mode === "operator"
        ? "memory.insights.toast_operator"
        : "memory.insights.toast_builder";
      const actionKey = mode === "operator"
        ? "memory.insights.toast_action_operator"
        : "memory.insights.toast_action_builder";

      addToast(
        get(t)(toastKey, { values: { count: insights.length } }),
        "info",
        {
          duration: EXTRACTION_TOAST_DURATION_MS,
          actionLabel: get(t)(actionKey),
          onaction: openPanel,
          "data-testid": "extraction-toast",
        },
      );
    }
    previousLength = insights.length;
  });
</script>

<InsightsFeedback open={feedbackOpen} onclose={closePanel} />
