<script lang="ts">
  /**
   * LoadedModelCard - one model resident in memory: whether it is ready or
   * answering, its kind, its format, whether it is kept in memory, its name
   * and what it can do. The screen that shows it says which facts and which
   * actions go with it.
   */
  import { t } from "svelte-i18n";
  import type { Snippet } from "svelte";
  import type { LoadedModel } from "$lib/types";
  import CapabilityIcon from "./CapabilityIcon.svelte";
  import ExplainedBadge from "./ExplainedBadge.svelte";
  import ServedCard from "./ServedCard.svelte";
  import { capabilitiesOf } from "./explanations";
  import { canUnload } from "./loadedModels";
  import { loadedFormat, loadedKindExplanationKey, loadedKindKey } from "./aiModels";

  interface Props {
    model: LoadedModel;
    /** Whether the model is kept in memory. */
    kept: boolean;
    /** The kind as the screen words it; the kind of the AI screen when absent. */
    kindLabel?: string;
    facts?: Snippet;
    actions?: Snippet;
    footer?: Snippet;
  }

  let { model, kept, kindLabel, facts, actions, footer }: Props = $props();

  const answering = $derived(!canUnload(model));
</script>

<ServedCard testid="ai-loaded-card" {facts} {actions} {footer}>
  {#snippet badges()}
    {#if answering}
      <ExplainedBadge
        label={$t("ai.loaded.answering")}
        explanation={$t("ai.loaded.answering_explanation")}
        variant="warning"
        testid="ai-loaded-answering"
      />
    {:else}
      <ExplainedBadge
        label={$t("ai.loaded.ready")}
        explanation={$t("ai.loaded.ready_explanation")}
        variant="success"
        testid="ai-loaded-ready"
      />
    {/if}
    <ExplainedBadge
      label={kindLabel ?? $t(loadedKindKey(model))}
      explanation={$t(loadedKindExplanationKey(model))}
      testid="ai-loaded-kind"
    />
    <ExplainedBadge
      label={loadedFormat(model).toUpperCase()}
      explanation={$t("ai.format_explanation")}
      help
      testid="ai-loaded-format"
    />
    {#if kept}
      <ExplainedBadge
        label={$t("ai.loaded.kept")}
        explanation={$t("ai.loaded.kept_explanation")}
        variant="primary"
        testid="ai-loaded-kept"
      />
    {/if}
  {/snippet}
  {#snippet identity()}
    <span class="truncate text-sm font-medium text-foreground" data-testid="ai-loaded-name">
      {model.modelId ?? model.model}
    </span>
    {#each capabilitiesOf(model.capabilities) as capability (capability)}
      <CapabilityIcon {capability} />
    {/each}
  {/snippet}
</ServedCard>
