<script lang="ts">
  import { onMount } from "svelte";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import EngineList from "../../components/settings/system/EngineList.svelte";
  import { enginesStore, settingsLoaders } from "$lib/stores/settings";
  import { developerMode } from "$lib/stores/developerMode";
  import { changeEngineChannel, verifyEngineUpdates } from "$lib/settings/engineChannel";
  import type { EngineChannel } from "$lib/ipc/system";

  onMount(() => {
    void settingsLoaders.engines();
  });

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
    developer={$developerMode}
    {checking}
    {checkError}
    onchannel={selectChannel}
    oncheck={checkUpdates}
  />
</SettingsSubPage>
