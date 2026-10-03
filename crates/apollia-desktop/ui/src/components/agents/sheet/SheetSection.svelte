<script lang="ts">
  /**
   * SheetSection - one titled section of an agent sheet: a title, marked
   * "Modified" when its value is the user's rather than the package's, a
   * one-line legend saying what the section is for, an optional action on the
   * right, and its body in a card.
   */
  import type { Snippet } from "svelte";
  import { t } from "svelte-i18n";
  import { Card } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";

  interface Props {
    title: string;
    legend: string;
    testid: string;
    /** Whether the value shown is the user's rather than the package's. */
    modified?: boolean;
    action?: Snippet;
    children: Snippet;
  }

  let { title, legend, testid, modified = false, action, children }: Props = $props();
</script>

<section class="space-y-2" data-testid={testid} aria-label={title}>
  <div class="flex items-start justify-between gap-3">
    <div class="min-w-0">
      <div class="flex items-center gap-2">
        <h3 class="m-0 text-body-sm font-semibold text-foreground">{title}</h3>
        {#if modified}
          <Badge size="sm" variant="warning" data-testid={`${testid}-modified`}>{$t("agent_adjust.modified")}</Badge>
        {/if}
      </div>
      <p class="m-0 text-caption text-muted-foreground">{legend}</p>
    </div>
    {#if action}
      <div class="shrink-0">{@render action()}</div>
    {/if}
  </div>
  <Card class="p-3.5">
    {@render children()}
  </Card>
</section>
