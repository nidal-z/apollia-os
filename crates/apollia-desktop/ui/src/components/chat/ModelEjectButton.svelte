<script lang="ts">
  /**
   * The eject button beside the selector of a conversation: it unloads the
   * local model the conversation talks to, and is greyed out when that model
   * is not in memory.
   */
  import { t } from "svelte-i18n";
  import { CircleArrowUp } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import { addToast } from "$lib/components/ui/toast";
  import { unloadLoadedModel } from "$lib/ipc/llm";
  import { canEject, ejectModel } from "$lib/chat/modelLoad";
  import { refreshModelResidency } from "$lib/stores/modelResidency";
  import { unloadErrorKey } from "../ai/loadedModels";
  import type { LoadedModel } from "$lib/types";

  interface Props {
    /** The resident model of the conversation, `null` when none is loaded. */
    model: LoadedModel | null;
  }

  let { model }: Props = $props();

  let ejecting = $state(false);
  const ready = $derived(canEject(model) && !ejecting);
  const hint = $derived(
    model === null
      ? $t("chat.model_eject.nothing_loaded")
      : canEject(model)
        ? $t("chat.model_eject.label")
        : $t("chat.model_eject.answering"),
  );

  async function eject(): Promise<void> {
    if (!ready) return;
    ejecting = true;
    const attempt = await ejectModel(unloadLoadedModel, model);
    ejecting = false;
    await refreshModelResidency();
    if (attempt.kind === "nothing") return;
    const name = attempt.model.modelId ?? attempt.model.model;
    if (attempt.kind === "ejected") {
      addToast($t("ai.loaded.unloaded_toast", { values: { name } }), "success");
      return;
    }
    const key = unloadErrorKey(attempt.message);
    addToast(
      key
        ? $t(key, { values: { name } })
        : $t("chat.model_eject.failed", { values: { name, error: attempt.message } }),
      "error",
    );
  }
</script>

<button
  type="button"
  class="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-surface-2 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-transparent"
  onclick={() => void eject()}
  disabled={!ready}
  aria-label={$t("chat.model_eject.label")}
  title={hint}
  data-testid="chat-model-eject"
>
  {#if ejecting}
    <Spinner size={13} />
  {:else}
    <CircleArrowUp size={14} strokeWidth={1.5} />
  {/if}
</button>
