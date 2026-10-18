<script lang="ts">
  /**
   * BatchSynthesisCard - what the agent wrote once every item of the batch
   * had run, when the launch asked for it: the text, and the file it was
   * written to beside the source; while it is written, a line that says so.
   */
  import { t } from "svelte-i18n";
  import { Card } from "$lib/components/operator";
  import { MarkdownContent } from "$lib/components/ui/markdown";
  import { Spinner } from "$lib/components/ui/progress";
  import type { SynthesisView } from "./batchView";

  interface Props {
    synthesis: SynthesisView;
  }

  let { synthesis }: Props = $props();
</script>

<Card class="space-y-2 p-4" data-testid="batch-synthesis">
  <h3 class="m-0 text-caption font-medium uppercase tracking-wide text-muted-foreground">
    {$t("batch_view.synthesis.title")}
  </h3>
  {#if synthesis.state === "completed"}
    <div class="text-body-sm text-foreground" data-testid="batch-synthesis-text">
      <MarkdownContent content={synthesis.text} />
    </div>
    {#if synthesis.path !== null}
      <p class="m-0 break-words text-caption text-muted-foreground" data-testid="batch-synthesis-path">
        {$t("batch_view.synthesis.written", { values: { path: synthesis.path } })}
      </p>
    {/if}
  {:else if synthesis.state === "failed"}
    <p class="m-0 break-words text-body-sm text-danger-a11y" data-testid="batch-synthesis-failed">
      {$t("batch_view.synthesis.failed", { values: { cause: synthesis.cause } })}
    </p>
  {:else}
    <p class="m-0 flex items-center gap-1.5 text-body-sm text-foreground">
      <Spinner size={12} class="shrink-0" />
      {$t(synthesis.state === "running" ? "batch_view.synthesis.running" : "batch_view.synthesis.waiting")}
    </p>
  {/if}
</Card>
