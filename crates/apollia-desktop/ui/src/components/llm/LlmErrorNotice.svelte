<script lang="ts">
  /**
   * LlmErrorNotice - humanized load/reload failure banner.
   *
   * Operator gets a plain-language reason and a retry. Builder additionally gets
   * the raw error text behind a Disclosure. Copy comes from `humanize` via
   * `reportError` upstream; this component only presents the `HumanizedError`.
   */
  import { t } from "svelte-i18n";
  import type { HumanizedError } from "$lib/errors/humanize";
  import { uiMode } from "$lib/stores/mode";
  import { ErrorBanner } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { Disclosure } from "$lib/components/ui/disclosure";

  interface Props {
    error: HumanizedError;
    onretry: () => void;
    onOpenSettings: () => void;
    loading?: boolean;
  }

  let { error, onretry, onOpenSettings, loading = false }: Props = $props();

  const isBuilder = $derived($uiMode === "builder");
</script>

<ErrorBanner
  {onretry}
  {loading}
  retryLabel={$t("common.retry")}
  data-testid="llm-error"
>
  <div class="flex flex-col gap-1.5">
    <span class="font-semibold text-foreground">{error.title}</span>
    <span class="text-muted-foreground">{error.friendly_message}</span>
    <span class="text-muted-foreground">{error.suggested_action}</span>

    {#if isBuilder && error.detail}
      <div class="mt-1">
        <Disclosure testid="llm-error-details">
          {#snippet summary()}{$t("llm.details")}{/snippet}
          <pre class="whitespace-pre-wrap break-words font-mono text-body-xs text-foreground/80">{error.detail}</pre>
        </Disclosure>
      </div>
    {/if}

    {#if !isBuilder}
      <div class="mt-1">
        <Button variant="ghost" size="sm" onclick={onOpenSettings}>
          {$t("llm.open_settings")}
        </Button>
      </div>
    {/if}
  </div>
</ErrorBanner>
