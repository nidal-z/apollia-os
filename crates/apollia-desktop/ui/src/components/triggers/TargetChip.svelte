<script lang="ts">
  /**
   * The typed target of an automation: the kind (agent or workflow) with its
   * icon, then the name. Used by the list, the journal and the creation
   * dialog, so the three read the target the same way.
   */
  import { t } from "svelte-i18n";
  import { Bot, Workflow } from "lucide-svelte";
  import type { TriggerTargetKind } from "$lib/types";
  import { targetKindLabelKey } from "./triggerTarget";

  interface Props {
    kind: TriggerTargetKind;
    name: string;
    testid?: string;
  }

  let { kind, name, testid }: Props = $props();
</script>

<span
  class="inline-flex min-w-0 items-center gap-1.5 text-caption text-muted-foreground"
  data-testid={testid}
  data-target-kind={kind}
>
  <span
    class="inline-flex shrink-0 items-center gap-1 rounded-sm border border-border bg-muted/40 px-1.5 py-px font-medium text-foreground"
  >
    {#if kind === "workflow"}
      <Workflow size={11} aria-hidden="true" />
    {:else}
      <Bot size={11} aria-hidden="true" />
    {/if}
    {$t(targetKindLabelKey(kind))}
  </span>
  <span class="truncate" title={name}>{name}</span>
</span>
