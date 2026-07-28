<script lang="ts">
  /**
   * SttPerformanceSection - recording ceiling, text insertion mode, clipboard
   * restore, and the detected hardware acceleration badge. Mutates the shared
   * `config` proxy in place.
   */
  import { t } from "svelte-i18n";
  import { SlidersHorizontal, Sparkles } from "lucide-svelte";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";
  import { Toggle } from "$lib/components/ui/toggle";
  import SettingsSection from "../SettingsSection.svelte";
  import SettingsFieldRow from "../SettingsFieldRow.svelte";
  import { sttStatus } from "$lib/stores/stt";
  import type { SttConfigView } from "$lib/types";

  interface Props {
    config: SttConfigView;
  }

  let { config }: Props = $props();

  const accelerationLabel = $derived(
    $sttStatus?.metal_enabled
      ? $t("settings.stt_metal")
      : $sttStatus?.cuda_enabled
        ? $t("settings.stt_cuda")
        : null,
  );
</script>

<SettingsSection
  title={$t('settings.stt.section.performance_title')}
  description={$t('settings.stt.section.performance_description')}
  bodyLayout="flush"
  data-testid="stt-section-performance"
>
  {#snippet icon()}<SlidersHorizontal size={15} strokeWidth={1.75} />{/snippet}

  <SettingsFieldRow label={$t('settings.stt_max_recording')} for="stt-max-rec">
    {#snippet control()}
      <div class="relative w-32">
        <Input id="stt-max-rec" data-testid="stt-max-rec" type="number" min="5" max="300" bind:value={config.max_recording_sec} class="pr-7" />
        <span class="pointer-events-none absolute inset-y-0 right-3 flex items-center text-caption text-muted-foreground">s</span>
      </div>
    {/snippet}
  </SettingsFieldRow>

  <SettingsFieldRow label={$t('settings.stt_clipboard_mode')} for="stt-clipboard" layout="stack">
    {#snippet control()}
      <Select id="stt-clipboard" data-testid="stt-clipboard" bind:value={config.clipboard_mode} class="text-sm">
        <option value="paste">{$t('settings.stt_clipboard_paste')}</option>
        <option value="clipboard">{$t('settings.stt_clipboard_clipboard')}</option>
      </Select>
    {/snippet}
  </SettingsFieldRow>

  <SettingsFieldRow label={$t('settings.stt_clipboard_restore')} hint={$t('settings.stt.clipboard_restore_hint')}>
    {#snippet control()}
      <Toggle
        checked={config.clipboard_restore}
        onchange={(v) => (config.clipboard_restore = v)}
        aria-label={$t('settings.stt_clipboard_restore')}
        data-testid="stt-clipboard-restore"
      />
    {/snippet}
  </SettingsFieldRow>

  <SettingsFieldRow label={$t('settings.stt_acceleration')} hint={$t('settings.stt.acceleration_hint')} border={false}>
    {#snippet control()}
      {#if accelerationLabel}
        <span class="inline-flex items-center gap-1 rounded-full bg-primary/10 px-2 py-0.5 text-caption font-medium text-primary">
          <Sparkles size={12} strokeWidth={2} />
          {accelerationLabel}
        </span>
      {:else}
        <span class="text-body-sm text-muted-foreground">{$t('settings.stt_none')}</span>
      {/if}
    {/snippet}
  </SettingsFieldRow>
</SettingsSection>
