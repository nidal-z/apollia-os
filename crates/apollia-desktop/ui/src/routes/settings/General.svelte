<script lang="ts">
  import { onMount } from "svelte";
  import { t, locale } from "svelte-i18n";
  import { RefreshCw, Globe, FolderOpen, LifeBuoy, AlertTriangle, Lock } from "lucide-svelte";
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
  import { developerMode } from "$lib/stores/developerMode";
  import { openLogsDir } from "$lib/ipc/app";
  import { addToast } from "$lib/components/ui/toast";
  import {
    UPDATE_CHANNELS,
    hydrateUpdateChannel,
    isUpdateChannel,
    setUpdateChannel,
    updateChannel,
    updateChannelSelectState,
    updateChannelSource,
    type UpdateChannel,
  } from "$lib/stores/updateChannel";

  const ISSUES_URL = "https://github.com/Apollia-OS/apollia-os/issues/new";

  const version = $derived($systemInfoStore.data?.version ?? null);
  // Resolved by the runtime from the real home directory, never a literal
  // "~/.apollia": the two differ as soon as the process runs under another HOME.
  const dataDir = $derived($systemInfoStore.data?.data_dir ?? null);

  // The channel is the setting in force, never guessed from the version string.
  const channelLabel = $derived(
    $updateChannel === "preview"
      ? $t("settings.about.channel_prerelease")
      : $t("settings.about.channel_stable"),
  );
  const channelSelect = $derived(updateChannelSelectState($updateChannelSource));

  const CHANNEL_LABEL_KEYS: Record<UpdateChannel, string> = {
    stable: "settings.general.channel_stable",
    preview: "settings.general.channel_preview",
  };

  function selectChannel(value: string): void {
    if (isUpdateChannel(value)) void setUpdateChannel(value);
  }

  const currentLang = $derived($locale?.startsWith("fr") ? "fr" : "en");

  function selectLocale(value: string) {
    locale.set(value);
    setLocale(value);
  }

  async function openLogs(): Promise<void> {
    try {
      await openLogsDir();
    } catch {
      addToast($t("settings.general.logs_failed"), "error");
    }
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
    void hydrateUpdateChannel();
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
    <SettingsFieldRow
      label={$t("settings.general.channel_label")}
      hint={$t("settings.general.channel_hint")}
      for="general-update-channel"
      border={false}
      data-testid="general-update-channel"
    >
      {#snippet control()}
        <div class="flex flex-col items-end gap-1">
          <div class="flex flex-wrap items-center justify-end gap-2">
            <UpdateCheckPanel variant="bare" />
            <Select
              id="general-update-channel"
              size="sm"
              value={$updateChannel}
              disabled={channelSelect.disabled}
              onchange={(e) => selectChannel(e.currentTarget.value)}
              aria-label={$t("settings.general.channel_label")}
              data-testid="update-channel-select"
            >
              {#each UPDATE_CHANNELS as channel (channel)}
                <option value={channel}>{$t(CHANNEL_LABEL_KEYS[channel])}</option>
              {/each}
            </Select>
          </div>
          {#if channelSelect.locked}
            <p
              class="inline-flex items-center gap-1.5 text-xs text-muted-foreground"
              title={$t("settings.policy_locked_hint")}
              data-testid="update-channel-policy"
            >
              <Lock class="h-3 w-3" aria-hidden="true" />
              {$t("settings.policy_locked")}
            </p>
          {/if}
        </div>
      {/snippet}
    </SettingsFieldRow>
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
    {#if $developerMode}
      <SettingsFieldRow
        label={$t("settings.general.logs_label")}
        hint={$t("settings.general.logs_hint")}
        data-testid="general-logs"
      >
        {#snippet control()}
          <Button variant="outline" size="sm" onclick={openLogs} data-testid="general-open-logs">
            {$t("settings.general.logs_button")}
          </Button>
        {/snippet}
      </SettingsFieldRow>
    {/if}
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
