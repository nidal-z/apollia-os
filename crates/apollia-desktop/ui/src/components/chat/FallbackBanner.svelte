<script lang="ts">
  /**
   * Discreet banner announcing an LLM provider fallback (US-SP42-040).
   *
   * Rendered in response to `LlmFallbackTriggered` events emitted by the
   * router. Designed to stay out of the way: single line, muted colors, no
   * action buttons.
   */

  import type { LlmFallback } from "$lib/types";
  import { Shuffle } from "lucide-svelte";
  import { t } from "svelte-i18n";

  interface Props {
    fallback: LlmFallback;
  }

  let { fallback }: Props = $props();
</script>

<div
  class="flex items-start gap-1.5 rounded-md border border-amber-500/30 bg-amber-500/5 px-2 py-1"
  role="status"
  data-testid="llm-fallback-banner"
>
  <Shuffle class="h-3 w-3 flex-shrink-0 mt-0.5 text-amber-600 dark:text-amber-500" />
  <p class="text-[11px] leading-tight text-amber-800 dark:text-amber-300">
    {$t("chat.llm_fallback.banner", {
      default: "Switched to {to} after {reason}",
      values: { to: fallback.to_provider, reason: fallback.reason },
    })}
    <span class="opacity-60">({fallback.from_provider})</span>
  </p>
</div>
