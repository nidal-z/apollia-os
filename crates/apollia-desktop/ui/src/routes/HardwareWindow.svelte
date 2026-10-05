<script lang="ts">
  /**
   * HardwareWindow - System > Hardware in a window of its own: the graphics
   * devices, the resource monitor and the load guardrails, the same
   * components the settings show, over the same settings. It stays open when
   * the settings close, and the monitor measures for as long as it is open.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Tooltip } from "bits-ui";
  import { ToastContainer } from "$lib/components/ui/toast";
  import { hardwareStore, settingsLoaders } from "$lib/stores/settings";
  import { resourceUsage } from "$lib/settings/resourceMonitor";
  import GpuDevices from "../components/settings/system/GpuDevices.svelte";
  import ResourceMonitor from "../components/settings/system/ResourceMonitor.svelte";
  import GuardrailsSetting from "../components/settings/model-defaults/GuardrailsSetting.svelte";

  onMount(() => {
    void settingsLoaders.hardware();
  });
</script>

<Tooltip.Provider delayDuration={200}>
  <main
    class="flex h-screen flex-col gap-4 overflow-y-auto bg-background p-6 text-foreground"
  >
    <h1 class="text-heading-md text-foreground">{$t("settings.nav.hardware")}</h1>
    <GpuDevices
      hardware={$hardwareStore.data}
      onreload={() => settingsLoaders.hardware(true)}
      windowed
    />
    <ResourceMonitor usage={$resourceUsage} />
    <GuardrailsSetting />
  </main>
  <ToastContainer />
</Tooltip.Provider>
