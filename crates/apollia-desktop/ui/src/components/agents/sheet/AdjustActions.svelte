<script lang="ts">
  /**
   * AdjustActions - the buttons of a sheet's header for its adjustment.
   *
   * Outside a session: "Adjust", which makes the whole sheet editable. In a
   * session: how many changes it holds, "Back to defaults" (every override
   * saved for the agent removed, its model with them, after a confirmation),
   * "Cancel" (the changes of the session dropped, nothing written) and "Save".
   */
  import { t } from "svelte-i18n";
  import { RotateCcw, Save, SlidersHorizontal } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { addToast } from "$lib/components/ui/toast/store";
  import { hasOverrides } from "./agentAdjust";
  import type { AgentAdjust } from "./useAgentAdjust.svelte";

  interface Props {
    adjust: AgentAdjust;
  }

  let { adjust }: Props = $props();

  let confirmingReset = $state(false);

  const count = $derived(adjust.writes.length);
  const canReset = $derived(hasOverrides(adjust.base) || count > 0);

  async function save(): Promise<void> {
    if (count === 0) {
      adjust.cancel();
      return;
    }
    const saved = await adjust.save();
    if (saved) addToast($t("agent_adjust.saved"), "success");
    else addToast(adjust.error ?? $t("agent_adjust.save_failed"), "error");
  }

  async function reset(): Promise<void> {
    const done = await adjust.reset();
    confirmingReset = false;
    if (done) addToast($t("agent_adjust.reset_done"), "success");
    else addToast(adjust.error ?? $t("agent_adjust.save_failed"), "error");
  }
</script>

{#if adjust.active}
  <div class="flex flex-wrap items-center justify-end gap-2">
    <Badge size="sm" variant="warning" data-testid="agent-adjust-pending">
      {$t("agent_adjust.pending", { values: { count } })}
    </Badge>
    <Button
      variant="ghost"
      size="sm"
      disabled={!canReset || adjust.saving || adjust.resetting}
      onclick={() => (confirmingReset = true)}
      data-testid="agent-adjust-reset"
    >
      {#snippet icon()}<RotateCcw size={12} />{/snippet}
      {$t("agent_adjust.reset")}
    </Button>
    <Button
      variant="outline"
      size="sm"
      disabled={adjust.saving || adjust.resetting}
      onclick={() => adjust.cancel()}
      data-testid="agent-adjust-cancel"
    >
      {$t("common.cancel")}
    </Button>
    <Button
      variant="primary-solid"
      size="sm"
      disabled={adjust.saving || adjust.resetting}
      onclick={() => void save()}
      data-testid="agent-adjust-save"
    >
      {#snippet icon()}<Save size={12} />{/snippet}
      {adjust.saving ? $t("agent_adjust.saving") : $t("agent_adjust.save")}
    </Button>
  </div>
{:else}
  <Button variant="outline" size="sm" onclick={() => void adjust.begin()} data-testid="agent-adjust-open">
    {#snippet icon()}<SlidersHorizontal size={12} />{/snippet}
    {$t("agent_sheet.adjust")}
  </Button>
{/if}

<ConfirmDialog
  open={confirmingReset}
  onclose={() => (confirmingReset = false)}
  onconfirm={() => void reset()}
  title={$t("agent_adjust.reset_title")}
  message={$t("agent_adjust.reset_message", { values: { count } })}
  confirmLabel={$t("agent_adjust.reset")}
  loading={adjust.resetting}
  data-testid="agent-adjust-reset-dialog"
/>
