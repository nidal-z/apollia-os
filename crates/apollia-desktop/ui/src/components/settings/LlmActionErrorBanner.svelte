<script lang="ts">
  /**
   * LlmActionErrorBanner - humanized failure banner for the settings LLM page.
   *
   * Presents a resolved `HumanizedError` (title + friendly message + suggested
   * action) with a retry, and keeps the raw error text behind a Disclosure so
   * the operator never sees a bare `os error 61`. Used for both the initial
   * list-load failure and reload / set-default failures.
   */
  import { t } from "svelte-i18n";
  import type { HumanizedError } from "$lib/errors/humanize";
  import { ErrorBanner } from "$lib/components/operator";
  import { Disclosure } from "$lib/components/ui/disclosure";

  interface Props {
    error: HumanizedError;
    onretry: () => void;
    loading?: boolean;
  }

  let { error, onretry, loading = false }: Props = $props();
</script>

<ErrorBanner {onretry} {loading} retryLabel={$t("common.retry")} data-testid="llm-action-error">
  <div class="flex flex-col gap-1.5">
    <span class="font-semibold text-foreground">{error.title}</span>
    <span class="text-muted-foreground">{error.friendly_message}</span>
    <span class="text-muted-foreground">{error.suggested_action}</span>
    {#if error.detail}
      <div class="mt-1">
        <Disclosure testid="llm-error-details">
          {#snippet summary()}{$t("llm.details")}{/snippet}
          <pre
            class="whitespace-pre-wrap break-words font-mono text-body-xs text-foreground/80"
          >{error.detail}</pre>
        </Disclosure>
      </div>
    {/if}
  </div>
</ErrorBanner>
