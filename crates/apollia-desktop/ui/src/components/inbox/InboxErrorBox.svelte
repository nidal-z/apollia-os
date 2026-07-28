<script lang="ts">
  /**
   * InboxErrorBox - humanized, never-swallowed error surface.
   *
   * Replaces the route's silent `.catch(() => [])` / bare error strings: a raw
   * IPC failure is run through `humanize()` into a plain-language cause and a
   * concrete recovery action, with the raw error chain kept behind a "Details"
   * disclosure (the builder reading). A retry action re-runs the failed load.
   *
   * Phase-0.5 local primitive (flagged): lives under `components/inbox/` until
   * promoted, per the ownership rules.
   */
  import { t } from "svelte-i18n";
  import { AlertTriangle, RotateCw } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import { humanize, type ErrorTranslate } from "$lib/errors/humanize";

  interface Props {
    /** Raw error (string / Error / unknown) - always surfaced, never hidden. */
    error: unknown;
    /** Re-run the failed operation. */
    onRetry?: () => void;
    /** Optional testid forwarded to the root. */
    testid?: string;
  }

  let { error, onRetry, testid }: Props = $props();

  const humanized = $derived(humanize(error, $t as ErrorTranslate));
</script>

<div
  class="flex items-start gap-3 rounded-xl border border-destructive/30 bg-destructive/[0.06] px-4 py-4"
  role="alert"
  data-testid={testid}
>
  <span
    class="inline-grid size-8 shrink-0 place-items-center rounded-md bg-destructive/[0.12] text-destructive"
    aria-hidden="true"
  >
    <AlertTriangle size={18} strokeWidth={2} />
  </span>
  <div class="min-w-0 flex-1">
    <h4 class="m-0 text-body-sm font-semibold text-foreground">{humanized.title}</h4>
    <p class="mt-1 max-w-[56ch] text-body-xs text-muted-foreground">
      {humanized.friendly_message}
    </p>
    <p class="mt-1 max-w-[56ch] text-body-xs text-muted-foreground">
      {humanized.suggested_action}
    </p>

    <div class="mt-2.5 flex flex-wrap items-center gap-2">
      {#if onRetry}
        <Button variant="primary-solid" size="sm" onclick={onRetry} data-testid="inbox-error-retry">
          {#snippet icon()}<RotateCw size={12} />{/snippet}
          {$t("common.retry")}
        </Button>
      {/if}
      {#if humanized.learn_more_url}
        <a
          class="rounded-md px-2.5 py-1 text-body-xs text-primary underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
          href={humanized.learn_more_url}
          target="_blank"
          rel="noreferrer"
        >
          {$t("inbox.error.learn_more")}
        </a>
      {/if}
    </div>

    {#if humanized.detail}
      <div class="mt-2.5">
        <Disclosure testid="inbox-error-details">
          {#snippet summary()}
            <span class="text-caption text-muted-foreground">{$t("inbox.error.details")}</span>
          {/snippet}
          {#snippet children()}
            <pre
              class="m-0 overflow-x-auto rounded-md border border-[hsl(var(--border-soft))] bg-[hsl(var(--code-bg))] px-3 py-2 font-mono text-caption leading-relaxed text-muted-foreground">{humanized.detail}</pre>
          {/snippet}
        </Disclosure>
      </div>
    {/if}
  </div>
</div>
