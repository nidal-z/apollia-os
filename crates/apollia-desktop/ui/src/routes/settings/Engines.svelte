<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import EngineList from "../../components/settings/system/EngineList.svelte";
  import { enginesStore, systemInfoStore, settingsLoaders } from "$lib/stores/settings";

  const LICENSE_SPDX = "MIT OR Apache-2.0";

  onMount(() => {
    void settingsLoaders.engines();
    void settingsLoaders.systemInfo();
  });

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
  <EngineList
    response={$enginesStore.data}
    loading={$enginesStore.loading}
    error={$enginesStore.error}
    {diagnosticHead}
  />
</SettingsSubPage>
