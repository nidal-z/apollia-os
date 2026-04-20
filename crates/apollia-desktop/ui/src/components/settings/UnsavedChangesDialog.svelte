<script lang="ts">
  import { tick } from "svelte";
  import { t } from "svelte-i18n";
  import Dialog from "$lib/components/ui/dialog/Dialog.svelte";
  import DialogFooter from "$lib/components/ui/dialog/DialogFooter.svelte";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    open: boolean;
    saving?: boolean;
    onDiscard: () => void;
    onSave: () => void;
    onStay: () => void;
  }

  let { open, saving = false, onDiscard, onSave, onStay }: Props = $props();

  $effect(() => {
    if (open) {
      tick().then(() => {
        const stay = document.querySelector<HTMLButtonElement>(
          '[data-testid="settings-unsaved-dialog-stay"]',
        );
        stay?.focus();
      });
    }
  });
</script>

<Dialog
  {open}
  onclose={onStay}
  size="sm"
  title={$t('settings.unsaved_dialog_title')}
  data-testid="settings-unsaved-dialog"
>
  <p class="text-sm text-muted-foreground">{$t('settings.unsaved_dialog_message')}</p>

  <DialogFooter>
    <Button
      variant="destructive"
      onclick={onDiscard}
      data-testid="settings-unsaved-dialog-discard"
    >
      {$t('settings.unsaved_dialog_discard')}
    </Button>
    <Button
      variant="default"
      onclick={onSave}
      disabled={saving}
      data-testid="settings-unsaved-dialog-save"
    >
      {saving ? $t('settings.stt_saving') : $t('settings.unsaved_dialog_save_continue')}
    </Button>
    <Button
      variant="outline"
      onclick={onStay}
      data-testid="settings-unsaved-dialog-stay"
    >
      {$t('settings.unsaved_dialog_stay')}
    </Button>
  </DialogFooter>
</Dialog>
