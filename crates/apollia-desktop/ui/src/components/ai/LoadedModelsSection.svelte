<script lang="ts">
  /**
   * LoadedModelsSection - the "Loaded" section of the AI screen: one card per
   * model resident in memory, its kind, format, memory and load time, and the
   * button that unloads it.
   */
  import { locale, t } from "svelte-i18n";
  import type { LoadedModel, LoadedModelsResponse } from "$lib/types";
  import { formatRelativeTime } from "$lib/utils";
  import { Button } from "$lib/components/ui/button";
  import CapabilityIcon from "./CapabilityIcon.svelte";
  import ExplainedBadge from "./ExplainedBadge.svelte";
  import { capabilitiesOf } from "./explanations";
  import { canUnload } from "./loadedModels";
  import {
    gigabytes,
    loadedFormat,
    loadedKindExplanationKey,
    loadedKindKey,
    loadedMemoryBytes,
  } from "./aiModels";

  interface Props {
    view: LoadedModelsResponse | null;
    error: string | null;
    /** Path of the model being unloaded, if any. */
    unloading: string | null;
    onunload: (model: LoadedModel) => void;
    onretry: () => void;
  }

  let { view, error, unloading, onunload, onretry }: Props = $props();

  const lang = $derived($locale ?? "fr");
  const models = $derived(view?.models ?? []);
  const totalMemory = $derived(
    models.reduce((sum, m) => sum + (loadedMemoryBytes(m) ?? 0), 0),
  );
</script>

<section class="space-y-3" data-testid="ai-loaded-section">
  <div class="flex items-baseline gap-3">
    <h2 class="text-sm font-semibold text-foreground">{$t("ai.loaded.title")}</h2>
    {#if models.length > 0}
      <span class="text-xs text-muted-foreground" data-testid="ai-loaded-summary">
        {$t("ai.loaded.summary", {
          values: { count: models.length, size: gigabytes(totalMemory, lang) },
        })}
      </span>
    {/if}
  </div>

  {#if error}
    <div
      class="flex items-center justify-between gap-3 rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-sm"
      data-testid="ai-loaded-error"
    >
      <span class="text-foreground">{$t("ai.loaded.error")}</span>
      <Button variant="outline" size="sm" onclick={onretry} data-testid="ai-loaded-retry">
        {$t("common.retry")}
      </Button>
    </div>
  {:else if view && !view.engineAvailable}
    <p class="text-sm text-muted-foreground" data-testid="ai-loaded-no-engine">
      {$t("ai.loaded.no_engine")}
    </p>
  {:else if view && models.length === 0}
    <p class="text-sm text-muted-foreground" data-testid="ai-loaded-empty">
      {$t("ai.loaded.empty")}
    </p>
  {:else}
    <div class="space-y-2">
      {#each models as item (item.modelPath)}
        {@const memory = loadedMemoryBytes(item)}
        {@const answering = !canUnload(item)}
        <div
          class="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border bg-card p-3"
          data-testid="ai-loaded-card"
        >
          <div class="min-w-0 space-y-1.5">
            <div class="flex flex-wrap items-center gap-1.5">
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
                label={$t(loadedKindKey(item))}
                explanation={$t(loadedKindExplanationKey(item))}
                testid="ai-loaded-kind"
              />
              <ExplainedBadge
                label={loadedFormat(item).toUpperCase()}
                explanation={$t("ai.format_explanation")}
                help
                testid="ai-loaded-format"
              />
            </div>
            <div class="flex items-center gap-2">
              <span class="truncate text-sm font-medium text-foreground" data-testid="ai-loaded-name">
                {item.modelId ?? item.model}
              </span>
              {#each capabilitiesOf(item.capabilities) as capability (capability)}
                <CapabilityIcon {capability} />
              {/each}
            </div>
          </div>
          <div class="flex shrink-0 items-center gap-4">
            {#if memory !== null}
              <span class="text-sm text-foreground" data-testid="ai-loaded-memory">
                {$t("ai.loaded.memory", { values: { size: gigabytes(memory, lang) } })}
              </span>
            {/if}
            <span class="text-xs text-muted-foreground" data-testid="ai-loaded-since">
              {$t("ai.loaded.since", { values: { when: formatRelativeTime(item.loadedAt, lang) } })}
            </span>
            <Button
              variant="outline"
              size="sm"
              disabled={answering || unloading !== null}
              onclick={() => onunload(item)}
              data-testid="ai-loaded-unload-btn"
            >
              {$t("ai.loaded.unload")}
            </Button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</section>
