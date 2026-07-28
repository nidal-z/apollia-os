<script lang="ts">
  /**
   * SttAdvancedSection - global hotkey (captured via the fullscreen dialog) and
   * the silence detection threshold. Mutates the shared `config` proxy in place.
   */
  import { t } from "svelte-i18n";
  import { Keyboard } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { addToast } from "$lib/components/ui/toast";
  import SettingsSection from "../SettingsSection.svelte";
  import SettingsFieldRow from "../SettingsFieldRow.svelte";
  import HotkeyDisplay from "../HotkeyDisplay.svelte";
  import HotkeyCaptureDialog from "../HotkeyCaptureDialog.svelte";
  import { formatCombo } from "$lib/keyboard/hotkeyCapture";
  import type { SttConfigView } from "$lib/types";

  interface Props {
    config: SttConfigView;
  }

  let { config }: Props = $props();

  let dialogOpen = $state(false);

  function onConfirm(combo: string) {
    dialogOpen = false;
    config.hotkey = combo;
    addToast(
      $t("settings.stt.hotkey.updated_toast", { values: { combo: formatCombo(combo) } }),
      "success",
    );
  }

  function onCancel(reason?: "escape" | "timeout" | "close") {
    dialogOpen = false;
    addToast(
      reason === "timeout"
        ? $t("settings.stt.hotkey.timeout_toast")
        : $t("settings.stt.hotkey.cancelled_toast"),
      "info",
    );
  }
</script>

<SettingsSection
  title={$t('settings.stt.section.advanced_title')}
  description={$t('settings.stt.section.advanced_description')}
  bodyLayout="flush"
  data-testid="stt-section-advanced"
>
  {#snippet icon()}<Keyboard size={15} strokeWidth={1.75} />{/snippet}

  <SettingsFieldRow label={$t('settings.stt.hotkey.label')} hint={$t('settings.stt.hotkey.hint')}>
    {#snippet control()}
      <HotkeyDisplay
        hotkey={config.hotkey}
        placeholder={$t('settings.stt.hotkey.preview_placeholder')}
        data-testid="stt-hotkey-preview"
      />
      <Button variant="outline" size="sm" onclick={() => (dialogOpen = true)} data-testid="stt-hotkey-change-btn">
        {$t('settings.stt.hotkey.change')}
      </Button>
    {/snippet}
  </SettingsFieldRow>

  <SettingsFieldRow
    label={$t('settings.stt_silence_threshold')}
    hint={$t('settings.stt.silence_hint')}
    for="stt-silence"
    border={false}
  >
    {#snippet control()}
      <div class="relative w-32">
        <Input id="stt-silence" data-testid="stt-silence" type="number" min="-80" max="0" step="1" bind:value={config.silence_threshold_db} class="pr-9" />
        <span class="pointer-events-none absolute inset-y-0 right-3 flex items-center text-caption text-muted-foreground">dB</span>
      </div>
    {/snippet}
  </SettingsFieldRow>
</SettingsSection>

<HotkeyCaptureDialog open={dialogOpen} onconfirm={onConfirm} oncancel={onCancel} />
