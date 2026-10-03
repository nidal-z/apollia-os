<script lang="ts">
  import { onMount } from "svelte";
  import { t, locale } from "svelte-i18n";
  import { RefreshCw, Globe, FolderOpen, LifeBuoy, AlertTriangle } from "lucide-svelte";
  import { systemInfoStore, settingsLoaders } from "$lib/stores/settings";
  import { setLocale } from "$lib/i18n";
  import { openExternalUrl } from "$lib/utils/externalLink";
  import { reportError } from "$lib/errors/reportError";
  import { Button } from "$lib/components/ui/button";
  import { Select } from "$lib/components/ui/select";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import SettingsSection from "../../components/settings/SettingsSection.svelte";
  import SettingsFieldRow from "../../components/settings/SettingsFieldRow.svelte";
  import UpdateCheckPanel from "../../components/settings/UpdateCheckPanel.svelte";
  import CopyValue from "../../components/settings/about/CopyValue.svelte";
  import AboutBlock from "../../components/settings/about/AboutBlock.svelte";
  import HelpBlock from "../../components/settings/help/HelpBlock.svelte";
  import DangerZone from "../../components/settings/DangerZone.svelte";

  const ISSUES_URL = "https://github.com/Apollia-OS/apollia-os/issues/new";

  const version = $derived($systemInfoStore.data?.version ?? null);
  // Resolved by the runtime from the real home directory, never a literal
  // "~/.apollia": the two differ as soon as the process runs under another HOME.
  const dataDir = $derived($systemInfoStore.data?.data_dir ?? null);

  // The channel is inferred from the version string's prerelease tag (the part
  // after the first "-"), never fabricated: no tag means a stable build.
  const channelLabel = $derived.by(() => {
    if (!version) return undefined;
    return version.includes("-")
      ? $t("settings.about.channel_prerelease")
      : $t("settings.about.channel_stable");
  });

  const currentLang = $derived($locale?.startsWith("fr") ? "fr" : "en");

  function selectLocale(value: string) {
    locale.set(value);
    setLocale(value);
  }

  async function reportProblem(): Promise<void> {
    try {
      await openExternalUrl(ISSUES_URL);
    } catch (err) {
      reportError(err, { surface: "toast" });
    }
  }

  onMount(() => {
    void settingsLoaders.systemInfo();
  });
</script>

<SettingsSubPage route="general" data-testid="general-section">
  <!-- Version and updates -->
  <SettingsSection
    title={$t("settings.general.updates_title")}
    bodyLayout="flush"
    data-testid="general-updates"
  >
    {#snippet icon()}<RefreshCw size={15} strokeWidth={1.75} />{/snippet}
    <SettingsFieldRow label={$t("settings.general.version_label")} hint={channelLabel}>
      {#snippet control()}
        {#if version}
          <CopyValue
            value={version}
            label={$t("settings.general.version_label")}
            data-testid="general-version"
          />
        {:else}
          <span class="text-body-sm text-muted-foreground">
            {$t("settings.about.version_unknown")}
          </span>
        {/if}
      {/snippet}
    </SettingsFieldRow>
    <div class="py-3">
      <UpdateCheckPanel variant="bare" />
    </div>
  </SettingsSection>

  <!-- Interface -->
  <SettingsSection
    title={$t("settings.general.interface_title")}
    bodyLayout="flush"
    data-testid="general-interface"
  >
    {#snippet icon()}<Globe size={15} strokeWidth={1.75} />{/snippet}
    <SettingsFieldRow
      label={$t("settings.language_label")}
      hint={$t("settings.general.language_hint")}
      for="general-language"
      border={false}
    >
      {#snippet control()}
        <Select
          id="general-language"
          size="sm"
          value={currentLang}
          onchange={(e) => selectLocale(e.currentTarget.value)}
          aria-label={$t("settings.language_label")}
          data-testid="language-select"
        >
          <option value="fr">{$t("settings.language_fr")}</option>
          <option value="en">{$t("settings.language_en")}</option>
        </Select>
      {/snippet}
    </SettingsFieldRow>
  </SettingsSection>

  <!-- Application -->
  <SettingsSection
    title={$t("settings.general.application_title")}
    bodyLayout="flush"
    data-testid="general-application"
  >
    {#snippet icon()}<FolderOpen size={15} strokeWidth={1.75} />{/snippet}
    <SettingsFieldRow
      label={$t("settings.about.kv_data_dir")}
      hint={$t("settings.about.data_desc")}
      data-testid="about-data"
    >
      {#snippet control()}
        {#if dataDir}
          <CopyValue
            value={dataDir}
            label={$t("settings.about.kv_data_dir")}
            data-testid="about-copy-data-dir"
          />
        {:else}
          <span class="text-body-sm text-muted-foreground" data-testid="about-data-dir-unknown">
            {$t("settings.about.data_dir_unknown")}
          </span>
        {/if}
      {/snippet}
    </SettingsFieldRow>
    <SettingsFieldRow
      label={$t("settings.about.report_title")}
      hint={$t("settings.general.report_hint")}
      border={false}
    >
      {#snippet control()}
        <Button variant="outline" size="sm" onclick={reportProblem} data-testid="about-link-report">
          {$t("settings.general.report_button")}
        </Button>
      {/snippet}
    </SettingsFieldRow>
  </SettingsSection>

  <!-- Help -->
  <h2 class="inline-flex items-center gap-1.5 px-1 pt-2 text-overline uppercase text-muted-foreground">
    <LifeBuoy size={13} strokeWidth={1.75} aria-hidden="true" />
    {$t("settings.general.help_title")}
  </h2>
  <HelpBlock />

  <!-- Danger zone -->
  <h2 class="inline-flex items-center gap-1.5 px-1 pt-2 text-overline uppercase text-destructive">
    <AlertTriangle size={13} strokeWidth={1.75} aria-hidden="true" />
    {$t("settings.nav.danger")}
  </h2>
  <DangerZone />

  <!-- About -->
  <AboutBlock />
</SettingsSubPage>
