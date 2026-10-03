<script lang="ts">
  /**
   * The settings window: the settings shown above the current screen, which
   * stays mounted underneath under the backdrop. The cross and Escape close it
   * through the unsaved-changes guard the settings install; the screen comes
   * back as it was left.
   */
  import { t } from "svelte-i18n";
  import Dialog from "$lib/components/ui/dialog/Dialog.svelte";
  import { settingsWindowOpen, requestCloseSettingsWindow } from "$lib/stores/settingsWindow";
  import Settings from "../../routes/Settings.svelte";

  function close(): void {
    requestCloseSettingsWindow();
  }
</script>

<Dialog
  open={$settingsWindowOpen}
  onclose={close}
  label={$t("settings.window.label")}
  hideClose
  class="flex h-[min(820px,90vh)] w-[min(1180px,94vw)] flex-col overflow-hidden"
  bodyClass="flex min-h-0 flex-1 flex-col"
  data-testid="settings-window"
>
  <Settings onclose={close} />
</Dialog>
