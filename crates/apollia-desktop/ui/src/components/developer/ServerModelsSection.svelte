<script lang="ts">
  /**
   * The models the local server serves: one card per model in memory, with
   * its identifier to copy, its size, how many requests it answers at once,
   * the button that ejects it and the way to its detail and settings. The
   * cards follow loads and unloads made anywhere.
   */
  import { onMount } from "svelte";
  import { locale, t } from "svelte-i18n";
  import { listen } from "@tauri-apps/api/event";
  import { ChevronRight, Copy } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import { addToast } from "$lib/components/ui/toast";
  import { developerMode } from "$lib/stores/developerMode";
  import { unloadLoadedModel } from "$lib/ipc/llm";
  import {
    askModelLoad,
    loadedModels,
    modelPins,
    refreshModelResidency,
  } from "$lib/stores/modelResidency";
  import type { LoadedModel } from "$lib/types";
  import LoadedModelCard from "../ai/LoadedModelCard.svelte";
  import { gigabytes } from "../ai/aiModels";
  import { canUnload, isKeptInMemory } from "../ai/loadedModels";
  import {
    copyIdentifier,
    detailAsk,
    ejectServed,
    followResidency,
    servedModelId,
    servedModelKind,
    servedModelParallel,
    servedModelSize,
    servedModels,
    type ResidencyEvent,
  } from "./servedItems";

  let ejecting = $state<string | null>(null);

  const lang = $derived($locale ?? "fr");
  const models = $derived(servedModels($loadedModels));

  onMount(() => {
    let live = true;
    let stop: (() => void) | null = null;
    void refreshModelResidency();
    void followResidency(
      (handler) => listen<ResidencyEvent>("runtime-event", (event) => handler(event.payload)),
      () => void refreshModelResidency(),
    ).then((unlisten) => {
      if (live) stop = unlisten;
      else unlisten();
    });
    return () => {
      live = false;
      stop?.();
    };
  });

  async function eject(model: LoadedModel): Promise<void> {
    if (ejecting) return;
    ejecting = model.modelPath;
    const notice = await ejectServed(unloadLoadedModel, refreshModelResidency, model);
    ejecting = null;
    if (notice) addToast($t(notice.key, { values: notice.values }), notice.tone);
  }

  async function copy(model: LoadedModel): Promise<void> {
    const done = await copyIdentifier(servedModelId(model), (text) =>
      navigator.clipboard.writeText(text),
    );
    addToast(
      $t(done ? "dev.developer.models.copied" : "dev.developer.models.copy_failed"),
      done ? "success" : "error",
    );
  }

  function detail(model: LoadedModel): void {
    const ask = detailAsk(model, () => void refreshModelResidency());
    if (ask) askModelLoad(ask);
  }
</script>

{#if $developerMode}
  <section class="space-y-3" data-testid="developer-models-section">
    <div class="flex items-baseline gap-2">
      <h3 class="text-sm font-semibold text-foreground">{$t("dev.developer.models.title")}</h3>
      {#if $loadedModels}
        <span class="text-xs text-muted-foreground" data-testid="developer-models-count">
          {models.length}
        </span>
      {/if}
    </div>

    {#if $loadedModels && !$loadedModels.engineAvailable}
      <p class="text-sm text-muted-foreground" data-testid="developer-models-no-engine">
        {$t("ai.loaded.no_engine")}
      </p>
    {:else if $loadedModels && models.length === 0}
      <p class="text-sm text-muted-foreground" data-testid="developer-models-empty">
        {$t("dev.developer.models.empty")}
      </p>
    {:else}
      <div class="space-y-2">
        {#each models as item (item.modelPath)}
          {@const size = servedModelSize(item)}
          {@const parallel = servedModelParallel(item)}
          {@const opens = item.modelId != null && item.modelId !== ""}
          <LoadedModelCard
            model={item}
            kept={isKeptInMemory($modelPins, item)}
            kindLabel={servedModelKind(item)}
          >
            {#snippet facts()}
              {#if size !== null}
                <span class="text-sm text-foreground" data-testid="developer-model-size">
                  {$t("dev.developer.models.size", { values: { size: gigabytes(size, lang) } })}
                </span>
              {/if}
              {#if parallel !== null}
                <span class="text-sm text-muted-foreground" data-testid="developer-model-parallel">
                  {$t("dev.developer.models.parallel", { values: { count: parallel } })}
                </span>
              {/if}
            {/snippet}
            {#snippet actions()}
              <Button
                variant="outline"
                size="sm"
                disabled={!canUnload(item) || ejecting !== null}
                loading={ejecting === item.modelPath}
                onclick={() => void eject(item)}
                data-testid="developer-model-eject"
              >
                {$t("dev.developer.models.eject")}
              </Button>
            {/snippet}
            {#snippet footer()}
              <Button
                variant="ghost"
                size="sm"
                onclick={() => void copy(item)}
                data-testid="developer-model-copy"
              >
                {#snippet icon()}<Copy size={14} aria-hidden="true" />{/snippet}
                {$t("dev.developer.models.copy_id")}
              </Button>
              <Tooltip content={$t("dev.developer.models.detail_unavailable")} disabled={opens}>
                {#snippet trigger(props)}
                  <span {...props} class="inline-flex">
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled={!opens}
                      onclick={() => detail(item)}
                      data-testid="developer-model-detail"
                    >
                      {$t("dev.developer.models.detail")}
                      <ChevronRight size={14} aria-hidden="true" />
                    </Button>
                  </span>
                {/snippet}
              </Tooltip>
            {/snippet}
          </LoadedModelCard>
        {/each}
      </div>
    {/if}
  </section>
{/if}
