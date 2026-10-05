<script lang="ts">
  /**
   * HardwareHeaderActions - the two actions of the header of Settings >
   * Hardware. "Copy the information" puts the description of the machine on
   * the clipboard: the version of the application, the operating system, the
   * processor, the memory, the graphics devices, the engines with their
   * version, and the paths in developer mode. "Community" opens the address
   * of the community in the browser of the system; nothing is requested until
   * that click.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Check, Copy, Globe } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast";
  import { reportError } from "$lib/errors/reportError";
  import { handleExternalLinkClick } from "$lib/utils/externalLink";
  import { COMMUNITY_URL } from "$lib/community";
  import { developerMode } from "$lib/stores/developerMode";
  import {
    configStore,
    enginesStore,
    hardwareStore,
    settingsLoaders,
    systemInfoStore,
  } from "$lib/stores/settings";
  import { hardwareInfoText } from "$lib/settings/hardwareDevices";
  import { archLabel } from "./HardwareCard.svelte";
  import { engineRows, variantLabel } from "./EngineList.svelte";

  let copied = $state(false);
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  onMount(() => {
    void settingsLoaders.engines();
    void settingsLoaders.systemInfo();
    void settingsLoaders.hardware();
    void settingsLoaders.config();
    return () => clearTimeout(copiedTimer);
  });

  const text = $derived.by(() => {
    const info = $systemInfoStore.data;
    const cpu = $t("settings.engines.variant_cpu");
    return hardwareInfoText(
      {
        version: info?.version ?? null,
        os: info?.os ?? null,
        arch: archLabel(info?.arch),
        hardware: $hardwareStore.data,
        engines: engineRows($enginesStore.data).map((row) => {
          const variant = variantLabel(row.variant, cpu);
          return { name: variant ? `${row.name} · ${variant}` : row.name, version: row.version };
        }),
        paths: $developerMode
          ? {
              python: info?.python_path ?? null,
              config: $configStore.data?.path ?? null,
              dataDir: info?.data_dir ?? null,
            }
          : null,
      },
      {
        os: $t("settings.system_os"),
        processor: $t("settings.hardware.processor"),
        cores: $t("settings.hardware.info_cores"),
        memory: $t("settings.hardware.ram"),
        gpu: $t("settings.hardware.gpu"),
        noGpu: $t("settings.hardware.none"),
        off: $t("settings.hardware.info_off"),
        engines: $t("settings.hardware.info_engines"),
        unknown: $t("settings.about.diag_unknown"),
        python: $t("settings.system_python"),
        config: $t("settings.config.path_label"),
        dataDir: $t("settings.about.diag_data_dir"),
      },
    );
  });

  async function copyInfo(): Promise<void> {
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
      addToast($t("settings.about.copy_success"), "success");
      clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => (copied = false), 1500);
    } catch (err) {
      reportError(err, { surface: "toast" });
    }
  }
</script>

<Button variant="outline" size="sm" onclick={copyInfo} data-testid="hardware-copy-info">
  {#snippet icon()}
    {#if copied}
      <Check size={13} strokeWidth={2.2} class="text-success" />
    {:else}
      <Copy size={13} strokeWidth={1.9} />
    {/if}
  {/snippet}
  {$t("settings.hardware.copy_info")}
</Button>
<a
  href={COMMUNITY_URL}
  rel="noreferrer"
  onclick={handleExternalLinkClick}
  class="inline-flex h-9 items-center justify-center gap-1.5 whitespace-nowrap rounded-md border border-border bg-transparent px-3 text-sm font-medium text-foreground ring-offset-background transition-all duration-fast hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-2 active:scale-[0.98]"
  data-testid="hardware-community-link"
>
  <Globe size={13} strokeWidth={1.9} aria-hidden="true" />
  {$t("settings.hardware.community")}
</a>
