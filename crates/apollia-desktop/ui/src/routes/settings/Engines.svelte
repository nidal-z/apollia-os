<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import EngineList from "../../components/settings/system/EngineList.svelte";
  import EngineSelection from "../../components/settings/system/EngineSelection.svelte";
  import {
    enginesStore,
    hardwareStore,
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

  onMount(() => {
    void settingsLoaders.engines();
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
</script>

<SettingsSubPage route="engines" data-testid="engines-section">
  <EngineList
    response={$enginesStore.data}
    loading={$enginesStore.loading}
    error={$enginesStore.error}
    {acceleratorKind}
    developer={$developerMode}
    {checking}
    {checkError}
    onchannel={selectChannel}
    oncheck={checkUpdates}
  />
  <EngineSelection
    response={$enginesStore.data}
    {acceleratorKind}
    settings={$engineSettings}
    onchoose={selectEngine}
    onautoupdate={toggleAutoUpdate}
  />
</SettingsSubPage>
