<script lang="ts">
  /**
   * What a pane of a divided chat shows when the load guardrail refuses the
   * model it asked for: the refusal stays in this pane, the others go on.
   *
   * "Pick another model" opens the selector of this pane. "Load anyway"
   * opens the loading window on that model, where a load beyond the limit
   * asks for an explicit confirmation before anything is sent.
   */
  import { locale, t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import type { PaneSeat } from "$lib/chat/chatPanes";
  import type { ChatTarget } from "$lib/chat/chatTargets";
  import { gb } from "$lib/chat/modelLoad";
  import { paneLoads } from "$lib/stores/chatPanes";
  import { requestChatTargetSelector } from "$lib/stores/chatTargetSelector";
  import { askModelLoad } from "$lib/stores/modelResidency";

  interface Props {
    pane: PaneSeat;
    /** Called with the model once it is loaded after all. */
    onpick: (target: ChatTarget) => void;
  }

  let { pane, onpick }: Props = $props();

  const refusals = paneLoads.refusals;
  const held = $derived($refusals[pane.id] ?? null);
  const lang = $derived($locale ?? "fr");

  function chooseAnother(): void {
    paneLoads.dismiss(pane.id);
    pane.activate?.();
    requestChatTargetSelector();
  }

  function loadAnyway(): void {
    const current = held;
    if (!current) return;
    paneLoads.dismiss(pane.id);
    askModelLoad({
      modelId: current.target.route,
      format: current.target.format,
      onloaded: () => onpick(current.target),
    });
  }
</script>

{#if held}
  <div class="px-4 pt-3">
    <div
      class="rounded-lg border border-destructive/60 bg-destructive/5 px-3.5 py-3"
      role="alert"
      data-testid="chat-pane-refusal"
    >
      <p class="text-body-xs font-medium text-destructive" data-testid="chat-pane-refusal-title">
        {$t("chat.panes.refusal_title", { values: { name: held.target.label } })}
      </p>
      <p class="mt-1 text-body-xs text-foreground">
        {$t("chat.panes.refusal_body", {
          values: {
            estimated: gb(held.refusal.estimatedGb, lang),
            budget: gb(held.refusal.budgetGb, lang),
            level: $t(`settings.model_defaults.guardrails.${held.refusal.level}`),
          },
        })}
      </p>
      <div class="mt-2.5 flex flex-wrap items-center gap-2">
        <Button
          variant="outline"
          size="sm"
          onclick={chooseAnother}
          data-testid="chat-pane-refusal-choose"
        >
          {$t("chat.panes.refusal_choose")}
        </Button>
        <Button variant="ghost" size="sm" onclick={loadAnyway} data-testid="chat-pane-refusal-load">
          {$t("chat.model_load.load_anyway")}
        </Button>
      </div>
    </div>
  </div>
{/if}
