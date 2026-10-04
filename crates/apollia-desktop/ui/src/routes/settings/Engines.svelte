<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import EngineList from "../../components/settings/system/EngineList.svelte";
  import EngineSelection from "../../components/settings/system/EngineSelection.svelte";
  import {
    enginesStore,
    hardwareStore,
    systemInfoStore,
    settingsLoaders,
  } from "$lib/stores/settings";
  import {
    chooseEngine,
    engineSettings,
    hydrateEngineSettings,
    setEngineAutoUpdate,
    type EngineChoice,
    type EngineFormat,
  } from "$lib/settings/engineSelection";
  import { addToast } from "$lib/components/ui/toast";
  import { developerMode } from "$lib/stores/developerMode";
  import { changeEngineChannel, verifyEngineUpdates } from "$lib/settings/engineChannel";
  import type { EngineChannel } from "$lib/ipc/system";

  const LICENSE_SPDX = "MIT OR Apache-2.0";

  onMount(() => {
    void settingsLoaders.engines();
    void settingsLoaders.systemInfo();
    void settingsLoaders.hardware();
    void hydrateEngineSettings();
  });

  const acceleratorKind = $derived($hardwareStore.data?.accelerator.kind ?? null);

  async function selectEngine(format: EngineFormat, choice: EngineChoice): Promise<void> {
    const written = await chooseEngine(format, choice);
    if (!written) addToast($t("settings.engines.selection_write_failed"), "error");
  }

  async function toggleAutoUpdate(on: boolean): Promise<void> {
    const written = await setEngineAutoUpdate(on);
    if (!written) addToast($t("settings.engines.selection_write_failed"), "error");
  }

  let checking = $state(false);
  let checkError = $state<string | null>(null);

  /** Run one channel action at a time and keep what it refused. */
  async function run(action: () => Promise<string | null>): Promise<void> {
    if (checking) return;
    checking = true;
    checkError = null;
    try {
      checkError = await action();
    } finally {
      checking = false;
    }
  }

  const selectChannel = (channel: EngineChannel) => run(() => changeEngineChannel(channel));
  const checkUpdates = () => run(verifyEngineUpdates);

  // The diagnostic block opens on the facts of this installation; the engine
  // lines that follow are the versions the engines stated.
  const diagnosticHead = $derived.by(() => {
    const info = $systemInfoStore.data;
    const unknown = $t("settings.about.diag_unknown");
    const lines = [
      `Apollia OS v${info?.version ?? unknown}`,
      `${$t("settings.about.diag_platform")}: ${info?.os ?? unknown}`,
    ];
    if (info?.python_path) lines.push(`Python: ${info.python_path}`);
    if (info?.data_dir) lines.push(`${$t("settings.about.diag_data_dir")}: ${info.data_dir}`);
    lines.push(`${$t("settings.about.diag_license")}: ${LICENSE_SPDX}`);
    return lines;
  });
</script>

<SettingsSubPage route="engines" data-testid="engines-section">
  <EngineSelection
    response={$enginesStore.data}
    {acceleratorKind}
    settings={$engineSettings}
    onchoose={selectEngine}
    onautoupdate={toggleAutoUpdate}
  />
  <EngineList
    response={$enginesStore.data}
    loading={$enginesStore.loading}
    error={$enginesStore.error}
    {acceleratorKind}
    {diagnosticHead}
    developer={$developerMode}
    {checking}
    {checkError}
    onchannel={selectChannel}
    oncheck={checkUpdates}
  />
</SettingsSubPage>
