<script lang="ts">
  /**
   * The line above the composer when no model is in memory: it loads the
   * last model loaded by hand again, under the settings it had. Cmd R on
   * macOS, Ctrl R elsewhere, does the same.
   *
   * A reload the runtime refuses opens the loading window on that model,
   * where the estimate and the refusal of the load guardrail show.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { RefreshCw } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import { addToast } from "$lib/components/ui/toast";
  import { MOD_LABEL } from "$lib/shortcuts/chatShortcuts";
  import { reloadLastLoadedModel } from "$lib/ipc/llmLoad";
  import { formFromSettings, reloadLast, showReloadLast } from "$lib/chat/modelLoad";
  import {
    askModelLoad,
    lastLoadedModel,
    loadedModels,
    refreshModelResidency,
    reloadLastRequests,
  } from "$lib/stores/modelResidency";

  let reloading = $state(false);
  const shown = $derived(showReloadLast($loadedModels, $lastLoadedModel));

  onMount(() => {
    void refreshModelResidency();
  });

  async function reload(): Promise<void> {
    const last = $lastLoadedModel;
    if (!shown || !last || reloading) return;
    reloading = true;
    const attempt = await reloadLast(reloadLastLoadedModel, last);
    reloading = false;
    await refreshModelResidency();
    if (attempt.kind === "loaded") {
      addToast(
        $t("chat.model_load.loaded_toast", { values: { name: attempt.outcome.modelId } }),
        "success",
      );
      return;
    }
    askModelLoad({
      modelId: last.modelId,
      format: null,
      settings: formFromSettings(last.load),
      notice: $t("chat.reload_last.refused", { values: { error: attempt.message } }),
    });
  }

  // The chat shortcut: each request reloads. The value the store holds when
  // the line mounts is not a request.
  let seenRequests: number | null = null;
  $effect(() => {
    const requests = $reloadLastRequests;
    if (seenRequests === null) {
      seenRequests = requests;
      return;
    }
    if (requests === seenRequests) return;
    seenRequests = requests;
    void reload();
  });
</script>

{#if shown && $lastLoadedModel}
  <div class="flex justify-center px-4 pb-2" data-testid="chat-reload-last">
    <button
      type="button"
      class="inline-flex max-w-full items-center gap-2 rounded-md px-2 py-1 text-caption text-muted-foreground transition-colors hover:bg-surface-2 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 disabled:cursor-not-allowed disabled:opacity-60"
      onclick={() => void reload()}
      disabled={reloading}
      data-testid="chat-reload-last-btn"
    >
      {#if reloading}
        <Spinner size={12} />
      {:else}
        <RefreshCw size={12} strokeWidth={1.75} class="shrink-0" />
      {/if}
      <span>{$t(reloading ? "chat.reload_last.reloading" : "chat.reload_last.label")}</span>
      <span class="truncate font-medium text-foreground" data-testid="chat-reload-last-model">
        {$lastLoadedModel.modelId}
      </span>
      <kbd class="hidden items-center gap-0.5 text-overline sm:flex">
        <span class="v4-hair rounded border bg-muted px-1 py-px">{MOD_LABEL}</span>
        <span class="v4-hair rounded border bg-muted px-1 py-px">R</span>
      </kbd>
    </button>
  </div>
{/if}
