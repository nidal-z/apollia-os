<script lang="ts">
  /**
   * LlmLoadedModels - the local models resident in memory, and unloading one.
   *
   * Hidden on a host without a local engine. A model answering a request keeps
   * its unload button disabled; a model that starts answering between the list
   * and the click is refused by the runtime, and the refusal is phrased here.
   */
  import { onMount } from "svelte";
  import { locale, t } from "svelte-i18n";
  import type { LoadedModelsResponse } from "$lib/types";
  import { listLoadedModels, unloadLoadedModel } from "$lib/ipc/llm";
  import { reportError } from "$lib/errors/reportError";
  import { addToast } from "$lib/components/ui/toast";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Card } from "$lib/components/ui/card";
  import { canUnload, showLoadedModels, unloadErrorKey } from "./loadedModels";

  let view = $state<LoadedModelsResponse | null>(null);
  let unloading = $state<string | null>(null);

  async function load(): Promise<void> {
    try {
      view = await listLoadedModels();
    } catch (err: unknown) {
      reportError(err, { surface: "toast" });
    }
  }

  async function unload(modelPath: string, name: string): Promise<void> {
    if (unloading) return;
    unloading = modelPath;
    try {
      await unloadLoadedModel(modelPath);
      addToast($t("llm.loaded.unloaded_toast", { values: { name } }), "success");
    } catch (err: unknown) {
      const key = unloadErrorKey(err instanceof Error ? err.message : String(err));
      if (key) {
        addToast($t(key, { values: { name } }), "error");
      } else {
        reportError(err, { surface: "toast" });
      }
    } finally {
      unloading = null;
      await load();
    }
  }

  function since(iso: string): string {
    return new Date(iso).toLocaleTimeString($locale ?? undefined, {
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  onMount(() => {
    void load();
  });
</script>

{#if showLoadedModels(view) && view}
  <section data-testid="llm-loaded-section">
    <h2 class="mb-3 text-sm font-medium uppercase tracking-wider text-muted-foreground">
      {$t("llm.loaded.title")}
    </h2>
    {#if view.models.length === 0}
      <p class="text-sm text-muted-foreground" data-testid="llm-loaded-empty">
        {$t("llm.loaded.empty")}
      </p>
    {:else}
      <Card class="divide-y divide-border">
        {#each view.models as item (item.modelPath)}
          <div class="flex items-center justify-between gap-3 p-3" data-testid="llm-loaded-row">
            <div class="min-w-0">
              <p class="truncate text-sm font-medium">{item.model}</p>
              <p class="text-xs text-muted-foreground">
                {$t("llm.loaded.since", { values: { time: since(item.loadedAt) } })}
              </p>
            </div>
            <div class="flex shrink-0 items-center gap-2">
              {#if !canUnload(item)}
                <Badge variant="warning" size="sm" data-testid="llm-loaded-in-use">
                  {$t("llm.loaded.in_use")}
                </Badge>
              {/if}
              <Button
                variant="outline"
                size="sm"
                disabled={!canUnload(item) || unloading !== null}
                onclick={() => unload(item.modelPath, item.model)}
                data-testid="llm-loaded-unload-btn"
              >
                {$t("llm.loaded.unload")}
              </Button>
            </div>
          </div>
        {/each}
      </Card>
    {/if}
  </section>
{/if}
