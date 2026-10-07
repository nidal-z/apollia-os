<script lang="ts">
  /**
   * "Load a model": the models of this computer that are not in memory, and,
   * for the one chosen, the loading window, where the load is estimated,
   * set and sent.
   */
  import { t } from "svelte-i18n";
  import { Plus } from "lucide-svelte";
  import { ActionMenu } from "$lib/components/ui/action-menu";
  import { Button } from "$lib/components/ui/button";
  import { developerMode } from "$lib/stores/developerMode";
  import { chatTargets, refreshChatTargets } from "$lib/stores/chatTargets";
  import { askModelLoad, loadedModels, refreshModelResidency } from "$lib/stores/modelResidency";
  import type { ModelTarget } from "$lib/chat/chatTargets";
  import ModelLoadDialog from "../chat/ModelLoadDialog.svelte";
  import { loadAsk, loadModelTestId, loadableModels } from "./localServer";

  let open = $state(false);
  let reading = $state(false);

  const models = $derived(loadableModels($chatTargets.groups, $loadedModels));

  $effect(() => {
    if (!open) return;
    let live = true;
    reading = true;
    void Promise.all([refreshChatTargets(), refreshModelResidency()]).then(() => {
      if (live) reading = false;
    });
    return () => {
      live = false;
    };
  });

  function pick(model: ModelTarget, close: () => void): void {
    close();
    askModelLoad(loadAsk(model, () => void refreshModelResidency()));
  }
</script>

{#if $developerMode}
  <ActionMenu bind:open align="end" class="min-w-[16rem]">
    {#snippet triggerSlot(triggerProps)}
      <Button
        {...triggerProps}
        variant="primary-solid"
        size="sm"
        data-testid="developer-load-model"
      >
        {#snippet icon()}<Plus size={14} aria-hidden="true" />{/snippet}
        {$t("dev.developer.server.load_model")}
      </Button>
    {/snippet}
    {#snippet body({ close })}
      {#if models.length === 0}
        <p class="px-2 py-1.5 text-xs text-muted-foreground" data-testid="developer-load-model-empty">
          {$t(reading ? "dev.developer.server.load_reading" : "dev.developer.server.load_empty")}
        </p>
      {:else}
        <ul class="flex flex-col gap-0.5" role="menu" data-testid="developer-load-model-list">
          {#each models as model (model.route)}
            <li role="none">
              <button
                type="button"
                role="menuitem"
                class="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-body-xs text-foreground transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                onclick={() => pick(model, close)}
                data-testid={loadModelTestId(model)}
              >
                <span class="flex-1 truncate">{model.label}</span>
                {#if model.format}
                  <span class="font-mono text-micro uppercase text-muted-foreground">
                    {model.format}
                  </span>
                {/if}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    {/snippet}
  </ActionMenu>
  <ModelLoadDialog />
{/if}
