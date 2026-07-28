<script lang="ts">
  /**
   * TasksLoadError - humanized failure state for the task list.
   *
   * Renders the operator-friendly copy from a `HumanizedError` (already routed
   * through `reportError`) with a retry button, and keeps the raw backend text
   * behind a collapsed `Disclosure` so the trace is never thrown at the user.
   */
  import { t } from "svelte-i18n";
  import { AlertTriangle, RefreshCw } from "lucide-svelte";
  import { EmptyState } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import type { HumanizedError } from "$lib/errors/humanize";

  interface Props {
    error: HumanizedError;
    onretry: () => void;
  }

  let { error, onretry }: Props = $props();
</script>

<div class="flex flex-col items-center" data-testid="tasks-load-error">
  <EmptyState
    tone="neutral"
    title={error.title}
    desc={error.friendly_message}
  >
    {#snippet icon()}<AlertTriangle size={22} class="text-destructive" />{/snippet}
    {#snippet action()}
      <Button variant="outline" size="sm" onclick={onretry} data-testid="tasks-load-retry">
        {#snippet icon()}<RefreshCw size={12} />{/snippet}
        {$t("common.retry")}
      </Button>
    {/snippet}
  </EmptyState>

  {#if error.detail}
    <div class="w-full max-w-xl px-6 pb-6">
      <Disclosure open={false} testid="tasks-load-error-detail">
        {#snippet summary()}
          <span class="text-body-xs text-muted-foreground">{$t("tasks.error_details")}</span>
        {/snippet}
        {#snippet children()}
          <pre
            class="whitespace-pre-wrap break-all px-3 py-2.5 font-mono text-destructive text-body-xs leading-relaxed">{error.detail}</pre>
        {/snippet}
      </Disclosure>
    </div>
  {/if}
</div>
