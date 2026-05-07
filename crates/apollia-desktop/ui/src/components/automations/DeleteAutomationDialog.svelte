<script lang="ts">
  /**
   * Confirmation dialog shown before deleting an automation (C.T.9).
   *
   * Surfaces the fire count so the operator understands the side-effect of
   * losing the automation ; offers a "don't ask again" opt-out that the caller
   * persists in `localStorage` (scope: `apollia.delete_automation.skip`).
   */
  import { t } from "svelte-i18n";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox";

  interface Props {
    open: boolean;
    fireCount: number;
    automationId: string;
    onclose: () => void;
    onconfirm: (skipNext: boolean) => void;
    loading?: boolean;
  }

  let { open, fireCount, automationId, onclose, onconfirm, loading = false }: Props = $props();
  let skipNext = $state(false);
</script>

<Dialog
  {open}
  {onclose}
  size="sm"
  title={$t("automations.delete_dialog.title")}
  data-testid="delete-automation-dialog"
>
  <div class="space-y-3">
    <p class="text-sm text-muted-foreground">
      {$t("automations.delete_dialog.message", {
        values: { id: automationId, count: fireCount },
      })}
    </p>
    <p class="text-xs text-muted-foreground/80">
      {$t("automations.delete_dialog.history_note")}
    </p>
    <label class="flex items-center gap-2 text-xs text-muted-foreground">
      <Checkbox bind:checked={skipNext} data-testid="delete-automation-skip" />
      {$t("automations.delete_dialog.skip_next")}
    </label>
  </div>

  <DialogFooter>
    <Button variant="outline" onclick={onclose} data-testid="delete-automation-cancel">
      {$t("common.cancel")}
    </Button>
    <Button
      variant="destructive"
      onclick={() => onconfirm(skipNext)}
      disabled={loading}
      data-testid="delete-automation-confirm"
    >
      {loading ? $t("triggers.deleting") : $t("automations.delete_dialog.confirm")}
    </Button>
  </DialogFooter>
</Dialog>
