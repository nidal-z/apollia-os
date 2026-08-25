<script lang="ts">
  /**
   * SttEssentialSection - the core STT surface: engine toggle, whisper model
   * (with native picker import), language hint, input device, and trigger mode.
   * Mutates the shared `config` proxy in place so the parent form tracks dirty.
   */
  import { open as openFilePicker } from "@tauri-apps/plugin-dialog";
  import { homeDir, join as pathJoin } from "@tauri-apps/api/path";
  import { t } from "svelte-i18n";
  import { Mic, Folder } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Select } from "$lib/components/ui/select";
  import { Toggle } from "$lib/components/ui/toggle";
  import { addToast } from "$lib/components/ui/toast";
  import SettingsSection from "../SettingsSection.svelte";
  import SettingsFieldRow from "../SettingsFieldRow.svelte";
  import { sttModelsStore, settingsLoaders } from "$lib/stores/settings";
  import { importModelFile } from "$lib/ipc/stt";
  import { buildModelOptions, selectedModelValue } from "$lib/stt/modelOptions";
  import type { SttConfigView } from "$lib/types";

  interface Props {
    config: SttConfigView;
    inputDevices: string[];
    noMicrophone: boolean;
  }

  let { config, inputDevices, noMicrophone }: Props = $props();

  // Autonyms are proper nouns, identical in every locale. The set matches the
  // codes documented in `reference/configuration.md` and recognised by the
  // model-name language detection on the Rust side; a code offered by one and
  // not the other would leave this field with a value no option carries, and a
  // select in that state displays nothing.
  // i18n-ignore-start: language autonyms, written in their own language
  const STT_LANGUAGES: ReadonlyArray<{ code: string; label: string }> = [
    { code: "fr", label: "Français" },
    { code: "en", label: "English" },
    { code: "es", label: "Español" },
    { code: "de", label: "Deutsch" },
    { code: "it", label: "Italiano" },
    { code: "pt", label: "Português" },
    { code: "nl", label: "Nederlands" },
    { code: "pl", label: "Polski" },
    { code: "ru", label: "Русский" },
    { code: "zh", label: "中文" },
    { code: "ja", label: "日本語" },
    { code: "ko", label: "한국어" },
    { code: "ar", label: "العربية" },
  ];
  // i18n-ignore-end

  // A persisted language outside the list above, or a microphone that has since
  // been unplugged, would otherwise leave its select blank. Listing the stored
  // value keeps both fields truthful about what is configured.
  const languageOptions = $derived(
    config.language && !STT_LANGUAGES.some((l) => l.code === config.language)
      ? [{ code: config.language, label: config.language }, ...STT_LANGUAGES]
      : STT_LANGUAGES,
  );
  const deviceOptions = $derived(
    config.input_device && !inputDevices.includes(config.input_device)
      ? [config.input_device, ...inputDevices]
      : inputDevices,
  );

  let importing = $state(false);

  // The configured model always gets an option, even when the directory scan
  // did not return it. A select handed a value that no option carries selects
  // nothing at all, which is what made this field render blank while the status
  // row beside it showed the loaded model correctly.
  const modelOptions = $derived(
    buildModelOptions($sttModelsStore.data ?? [], config.model_path),
  );
  const selectedModel = $derived(selectedModelValue(config.model_path));

  async function loadModel() {
    if (importing) return;
    importing = true;
    try {
      const modelsDir = await pathJoin(await homeDir(), ".apollia", "models");
      const selected = await openFilePicker({
        multiple: false,
        filters: [{ name: $t("settings.stt_model_filter"), extensions: ["bin", "gguf"] }],
        title: $t("settings.stt_load_model_title"),
        defaultPath: modelsDir,
      });
      if (!selected) return;
      const filePath =
        typeof selected === "string" ? selected : (selected as { path: string }).path;
      const dest = await importModelFile(filePath, ["bin", "gguf"]);
      await settingsLoaders.sttModels(true);
      const name = dest.split(/[\\/]/).pop() ?? "";
      if (name) config.model_path = `~/.apollia/models/${name}`;
      addToast($t("settings.stt_model_loaded_toast", { values: { name } }), "success");
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      importing = false;
    }
  }
</script>

<SettingsSection
  title={$t('settings.stt.section.essential_title')}
  description={$t('settings.stt.section.essential_description')}
  bodyLayout="flush"
  data-testid="stt-section-essential"
>
  {#snippet icon()}<Mic size={15} strokeWidth={1.75} />{/snippet}

  <SettingsFieldRow
    label={$t('settings.stt_enable_engine')}
    hint={$t('settings.stt.enable_hint')}
    data-testid="stt-enable-toggle"
  >
    {#snippet control()}
      <Toggle
        checked={config.enabled}
        onchange={(v) => (config.enabled = v)}
        aria-label={$t('settings.stt_enable_engine')}
        data-testid="stt-enable-switch"
      />
    {/snippet}
  </SettingsFieldRow>

  <SettingsFieldRow
    label={$t('settings.stt_select_model')}
    hint={$t('settings.stt_load_model_hint')}
    for="stt-model-select"
    layout="stack"
  >
    {#snippet control()}
      {#if modelOptions.length > 0}
        <Select
          id="stt-model-select"
          value={selectedModel}
          onchange={(e: Event) => (config.model_path = (e.currentTarget as HTMLSelectElement).value)}
          class="text-sm"
          data-testid="stt-model-select"
        >
          {#each modelOptions as option (option.value)}
            <option value={option.value}>{option.label}</option>
          {/each}
        </Select>
      {:else}
        <p class="rounded-md border border-border/60 px-3 py-2 text-body-sm text-muted-foreground">
          {$t('settings.stt_no_models')}
        </p>
      {/if}
      <Button
        variant="outline"
        size="sm"
        onclick={loadModel}
        disabled={importing}
        loading={importing}
        data-testid="stt-load-model-btn"
      >
        {#snippet icon()}<Folder size={14} strokeWidth={1.75} />{/snippet}
        {$t('settings.stt_load_model')}
      </Button>
    {/snippet}
  </SettingsFieldRow>

  <SettingsFieldRow
    label={$t('settings.stt_language')}
    hint={$t('settings.stt.language_hint')}
    for="stt-language"
    layout="stack"
  >
    {#snippet control()}
      <Select
        id="stt-language"
        data-testid="stt-language-select"
        value={config.language ?? ''}
        onchange={(e: Event) => (config.language = (e.currentTarget as HTMLSelectElement).value || null)}
        class="text-sm"
      >
        <option value="">{$t('settings.stt_language_auto')}</option>
        {#each languageOptions as lang (lang.code)}
          <option value={lang.code}>{lang.label}</option>
        {/each}
      </Select>
    {/snippet}
  </SettingsFieldRow>

  <SettingsFieldRow
    label={$t('settings.stt_input_device')}
    hint={$t('settings.stt.input_hint')}
    for="stt-input-device"
    layout="stack"
  >
    {#snippet control()}
      <Select
        id="stt-input-device"
        data-testid="stt-input-device"
        value={config.input_device ?? ''}
        onchange={(e: Event) => (config.input_device = (e.currentTarget as HTMLSelectElement).value || null)}
        disabled={noMicrophone}
        class="text-sm"
      >
        <option value="">{$t('settings.stt_input_device_default')}</option>
        {#each deviceOptions as device (device)}
          <option value={device}>{device}</option>
        {/each}
      </Select>
      {#if noMicrophone}
        <p class="flex items-start gap-2 rounded-md border border-warning/30 bg-warning/10 px-3 py-2 text-caption text-warning" data-testid="stt-no-microphone">
          {$t('settings.stt_no_microphone')}
        </p>
      {/if}
    {/snippet}
  </SettingsFieldRow>

  <SettingsFieldRow
    label={$t('settings.stt_trigger_mode')}
    for="stt-trigger"
    layout="stack"
    border={false}
  >
    {#snippet control()}
      <Select id="stt-trigger" data-testid="stt-trigger" bind:value={config.trigger_mode} class="text-sm">
        <option value="toggle">{$t('settings.stt_trigger_toggle')}</option>
        <option value="push-to-talk">{$t('settings.stt_trigger_push')}</option>
      </Select>
    {/snippet}
  </SettingsFieldRow>
</SettingsSection>
