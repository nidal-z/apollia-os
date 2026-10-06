<script lang="ts">
  /**
   * DeveloperEnginesSection - "Engines", in Settings > Developer.
   *
   * The log level of the local engine, the channel the engines follow, the
   * automatic update of the chosen engines and the automatic removal of the
   * least used ones. The channel and the automatic update are the settings
   * System > Engines shows: one value each, written by the same calls, read
   * through the same stores.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Cpu, Lock } from "lucide-svelte";
  import { Select } from "$lib/components/ui/select";
  import { addToast } from "$lib/components/ui/toast";
  import { enginesStore, settingsLoaders } from "$lib/stores/settings";
  import {
    ENGINE_LOG_LEVELS,
    ENGINE_LOG_LEVEL_LABEL_KEYS,
    engineLogLevel,
    engineLogLevelSource,
    hydrateEngineLogLevel,
    isEngineLogLevel,
    setEngineLogLevel,
  } from "$lib/stores/engineLogLevel";
  import {
    ENGINE_CHANNELS,
    ENGINE_CHANNEL_LABEL_KEYS,
    changeEngineChannel,
    isEngineChannel,
  } from "$lib/settings/engineChannel";
  import {
    engineSettings,
    hydrateEngineSettings,
    setEngineAutoUpdate,
  } from "$lib/settings/engineSelection";
  import { developerSettings, setDeveloperSetting } from "$lib/stores/developerSettings";
  import SettingsSection from "../SettingsSection.svelte";
  import SettingsFieldRow from "../SettingsFieldRow.svelte";
  import SettingsToggle from "../SettingsToggle.svelte";
  import { AUTO_PURGE_ROW, DEVELOPER_ENGINES_KEYS } from "./developerExtras";

  const levelLocked = $derived($engineLogLevelSource === "policy");
  const channel = $derived($enginesStore.data?.channel ?? null);
  const autoPurge = $derived($developerSettings[AUTO_PURGE_ROW.key]);

  let changingChannel = $state(false);

  onMount(() => {
    void hydrateEngineLogLevel();
    void hydrateEngineSettings();
    void settingsLoaders.engines();
  });

  async function chooseLevel(value: string) {
    if (!isEngineLogLevel(value)) return;
    const written = await setEngineLogLevel(value);
    if (!written) addToast($t("settings.developer.engines.write_failed"), "error");
  }

  async function chooseChannel(value: string) {
    if (!isEngineChannel(value) || changingChannel) return;
    changingChannel = true;
    try {
      const refusal = await changeEngineChannel(value);
      if (refusal !== null) {
        addToast($t(DEVELOPER_ENGINES_KEYS.channelFailed, { values: { reason: refusal } }), "error");
      }
    } finally {
      changingChannel = false;
    }
  }

  async function toggleAutoUpdate(on: boolean) {
    const written = await setEngineAutoUpdate(on);
    if (!written) addToast($t(DEVELOPER_ENGINES_KEYS.writeFailed), "error");
  }

  async function toggleAutoPurge(on: boolean) {
    const written = await setDeveloperSetting(AUTO_PURGE_ROW.key, on);
    if (!written) addToast($t(DEVELOPER_ENGINES_KEYS.writeFailed), "error");
  }
</script>

<SettingsSection
  title={$t(DEVELOPER_ENGINES_KEYS.title)}
  bodyLayout="flush"
  data-testid="developer-engines"
>
  {#snippet icon()}<Cpu size={15} strokeWidth={1.75} />{/snippet}
  <SettingsFieldRow
    label={$t("settings.developer.engines.log_level_label")}
    hint={$t("settings.developer.engines.log_level_hint")}
    for="developer-engine-log-level"
    data-testid="developer-engine-log-level"
  >
    {#snippet control()}
      <div class="flex flex-col items-end gap-1">
        <Select
          id="developer-engine-log-level"
          size="sm"
          value={$engineLogLevel}
          disabled={levelLocked}
          onchange={(e) => void chooseLevel(e.currentTarget.value)}
          aria-label={$t("settings.developer.engines.log_level_label")}
          data-testid="engine-log-level-select"
        >
          {#each ENGINE_LOG_LEVELS as level (level)}
            <option value={level}>{$t(ENGINE_LOG_LEVEL_LABEL_KEYS[level])}</option>
          {/each}
        </Select>
        {#if levelLocked}
          <p
            class="inline-flex items-center gap-1.5 text-xs text-muted-foreground"
            title={$t("settings.policy_locked_hint")}
            data-testid="engine-log-level-policy"
          >
            <Lock class="h-3 w-3" aria-hidden="true" />
            {$t("settings.policy_locked")}
          </p>
        {/if}
      </div>
    {/snippet}
  </SettingsFieldRow>

  <SettingsFieldRow
    label={$t(DEVELOPER_ENGINES_KEYS.channelLabel)}
    hint={$t(DEVELOPER_ENGINES_KEYS.channelHint)}
    for="developer-engine-channel-select"
    data-testid="developer-engine-channel"
  >
    {#snippet control()}
      <Select
        id="developer-engine-channel-select"
        size="sm"
        value={channel ?? ENGINE_CHANNELS[0]}
        disabled={channel === null || changingChannel}
        onchange={(e) => void chooseChannel(e.currentTarget.value)}
        aria-label={$t(DEVELOPER_ENGINES_KEYS.channelLabel)}
        data-testid="developer-engine-channel-select"
      >
        {#each ENGINE_CHANNELS as option (option)}
          <option value={option}>{$t(ENGINE_CHANNEL_LABEL_KEYS[option])}</option>
        {/each}
      </Select>
    {/snippet}
  </SettingsFieldRow>

  <div class="border-b border-border/60 py-1.5">
    <SettingsToggle
      id="developer-engine-auto-update-switch"
      label={$t(DEVELOPER_ENGINES_KEYS.autoUpdateLabel)}
      description={$t(DEVELOPER_ENGINES_KEYS.autoUpdateHint)}
      checked={$engineSettings.autoUpdate.value}
      source={$engineSettings.autoUpdate.source}
      onChange={(next) => void toggleAutoUpdate(next)}
      data-testid="developer-engine-auto-update"
    />
  </div>

  <div class="py-1.5">
    <SettingsToggle
      id="developer-engine-auto-purge-switch"
      label={$t(AUTO_PURGE_ROW.label)}
      description={$t(AUTO_PURGE_ROW.hint)}
      checked={autoPurge.value}
      source={autoPurge.source}
      onChange={(next) => void toggleAutoPurge(next)}
      data-testid="developer-engine-auto-purge"
    />
  </div>
</SettingsSection>
