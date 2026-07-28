<script lang="ts">
  // Humanized plan-launch failure.
  //
  // When the approval / launch IPC fails, the operator sees plain language
  // (what happened, that nothing started, how to recover) with the raw cause
  // kept behind a Details disclosure for support. Copy is resolved through the
  // deterministic `humanize` mapper (no LLM), fed the live `svelte-i18n`
  // formatter. Retry is a callback owned by the parent.
  import { t } from "svelte-i18n";
  import { X, RotateCcw } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import { humanize, type ErrorTranslate } from "$lib/errors/humanize";

  interface Props {
    /** Raw error thrown by the IPC call. */
    raw: unknown;
    /** Retry the approval / launch. */
    onRetry: () => void;
    /** Disables retry while a call is in flight. */
    isActing?: boolean;
  }

  let { raw, onRetry, isActing = false }: Props = $props();

  const humanized = $derived(humanize(raw, $t as ErrorTranslate));
</script>

<div
  class="mt-3 rounded-lg border border-destructive/35 bg-destructive/5 p-3.5"
  data-testid="plan-review-error"
  role="alert"
>
  <div class="flex items-center gap-2 text-body-sm font-semibold text-destructive">
    <X size={16} strokeWidth={2.2} aria-hidden="true" />
    <span>{$t("plan_mode.launch_failed_title")}</span>
  </div>

  <p class="mt-2 text-body-sm leading-relaxed text-foreground">
    {humanized.friendly_message}
  </p>
  {#if humanized.suggested_action}
    <p class="mt-1 text-body-xs text-muted-foreground">
      {humanized.suggested_action}
    </p>
  {/if}

  {#if humanized.detail}
    <div class="mt-2.5">
      <Disclosure testid="plan-review-error-detail">
        {#snippet summary()}
          <span class="text-caption text-muted-foreground">
            {$t("plan_mode.launch_failed_hint")}
          </span>
        {/snippet}
        {#snippet children()}
          <pre
            class="whitespace-pre-wrap break-words rounded-md bg-surface-2 px-2.5 py-2 font-mono text-caption text-muted-foreground">{humanized.detail}</pre>
        {/snippet}
      </Disclosure>
    </div>
  {/if}

  <div class="mt-3 flex gap-2">
    <Button
      variant="default"
      size="sm"
      disabled={isActing}
      onclick={onRetry}
      data-testid="plan-review-retry"
    >
      {#snippet icon()}
        <RotateCcw size={14} aria-hidden="true" />
      {/snippet}
      {$t("plan_mode.retry_approval")}
    </Button>
  </div>
</div>
