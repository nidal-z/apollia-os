<script lang="ts">
  /**
   * ProjectsLoadError - humanized failure state for the projects list.
   *
   * Renders the `HumanizedError` produced by `reportError`/`humanize` through
   * the operator `EmptyState`, offers a Retry button, and tucks the raw
   * technical `detail` behind a `Disclosure` so operators are not shown the
   * stack trace by default.
   */
  import { t } from "svelte-i18n";
  import { AlertTriangle, RefreshCw } from "lucide-svelte";
  import { EmptyState } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import type { HumanizedError } from "$lib/errors/humanize";

  interface Props {
    error: HumanizedError;
    onRetry: () => void;
  }

  let { error, onRetry }: Props = $props();
</script>

<div class="flex flex-col items-center" data-testid="projects-load-error">
  <EmptyState
    tone="neutral"
    title={error.title}
    desc={error.friendly_message}
  >
    {#snippet icon()}<AlertTriangle size={22} class="text-danger-a11y" />{/snippet}
    {#snippet action()}
      <div class="flex flex-col items-center gap-3">
        <Button
          variant="primary-solid"
          size="sm"
          onclick={onRetry}
          data-testid="projects-load-retry"
        >
          {#snippet icon()}<RefreshCw size={12} />{/snippet}
          {$t("common.retry")}
        </Button>
        {#if error.detail}
          <div class="w-full max-w-md">
            <Disclosure open={false} testid="projects-load-error-details">
              {#snippet summary()}
                <span class="text-body-sm font-medium text-foreground">
                  {$t("common.error_details")}
                </span>
              {/snippet}
              {#snippet children()}
                <p class="px-3 py-2 font-mono text-caption text-danger-a11y break-words">
                  {error.detail}
                </p>
              {/snippet}
            </Disclosure>
          </div>
        {/if}
      </div>
    {/snippet}
  </EmptyState>
</div>
