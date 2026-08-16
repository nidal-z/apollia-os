<script lang="ts" module>
  export const meta = {
    title: "settings.nav.stt",
    icon: "mic",
    group: "settings.nav.cluster_ai",
    cluster: "ai",
  } as const;
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import { t, locale } from "svelte-i18n";
  import { Activity, Info, RefreshCw } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { ErrorBanner } from "$lib/components/operator";
  import SettingSectionSkeleton from "../../components/settings/SettingSectionSkeleton.svelte";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import SettingsSection from "../../components/settings/SettingsSection.svelte";
  import SttEssentialSection from "../../components/settings/stt/SttEssentialSection.svelte";
  import SttAdvancedSection from "../../components/settings/stt/SttAdvancedSection.svelte";
  import SttPerformanceSection from "../../components/settings/stt/SttPerformanceSection.svelte";
  import SttTestCard from "../../components/settings/stt/SttTestCard.svelte";
  import SttStatusRows from "../../components/settings/stt/SttStatusRows.svelte";
  import { refreshSttStatus } from "$lib/stores/stt";
  import { sttConfigStore, settingsLoaders } from "$lib/stores/settings";
  import { useSettingsForm } from "$lib/settings/useSettingsForm";
  import { addToast } from "$lib/components/ui/toast";
  import { updateSttConfig, reloadStt, listAudioInputDevices } from "$lib/ipc/stt";
  import type { SttConfigView } from "$lib/types";

  let sttConfig = $state<SttConfigView | null>(null);
  let savedBaseline = $state<SttConfigView | null>(null);
  // `sttApplied` confirms the engine hot-reloaded; `sttNeedsRestart` is the
  // honest fallback shown only when the explicit reload after save fails.
  let sttApplied = $state(false);
  let sttNeedsRestart = $state(false);

  let inputDevices = $state<string[]>([]);
  let inputDevicesLoaded = $state(false);
  const noMicrophone = $derived(inputDevicesLoaded && inputDevices.length === 0);

  async function loadInputDevices() {
    try {
      inputDevices = await listAudioInputDevices();
    } catch {
      inputDevices = [];
    } finally {
      inputDevicesLoaded = true;
    }
  }

  type SttForm = ReturnType<typeof useSettingsForm<SttConfigView>>;
  let form: SttForm | null = null;

  $effect(() => {
    const loaded = $sttConfigStore.data;
    if (loaded && !sttConfig) {
      sttConfig = { ...loaded };
      savedBaseline = { ...loaded };
      form = useSettingsForm<SttConfigView>({
        route: "stt",
        initial: { ...loaded },
        autoSave: "explicit",
        onSave: async (values) => {
          sttApplied = false;
          sttNeedsRestart = false;
          await updateSttConfig(values);
          // `update_stt_config` reloads internally but swallows reload errors;
          // reload explicitly so a genuine hot-reload failure surfaces here.
          try {
            await reloadStt();
            await refreshStatus();
            sttApplied = true;
          } catch {
            sttNeedsRestart = true;
          }
        },
      });
    }
  });

  $effect(() => {
    if (sttConfig && form) form.sync($state.snapshot(sttConfig) as SttConfigView);
  });

  async function handleSave(): Promise<boolean> {
    if (!form) return false;
    const ok = await form.save();
    if (ok && sttConfig) savedBaseline = { ...($state.snapshot(sttConfig) as SttConfigView) };
    return ok;
  }

  function handleReset(): void {
    form?.reset();
    if (savedBaseline) sttConfig = { ...savedBaseline };
  }

  /**
   * Re-reads the persisted STT row into the form.
   *
   * `sttConfig` is copied out of the store exactly once (the `!sttConfig` guard
   * above), so without this the page keeps showing the values it opened with
   * even after the engine was rebuilt from different ones.
   */
  async function reloadPersistedConfig(): Promise<void> {
    await settingsLoaders.sttConfig(true);
    const loaded = $sttConfigStore.data;
    if (!loaded) return;
    sttConfig = { ...loaded };
    savedBaseline = { ...loaded };
    form?.sync({ ...loaded });
  }

  // ─── Status section ───────────────────────────────────────────────────
  let lastStatusRefresh = $state<number | null>(null);
  let liveUpdates = $state(false);
  let now = $state(Date.now());
  let reloading = $state(false);

  async function refreshStatus() {
    await refreshSttStatus();
    lastStatusRefresh = Date.now();
  }

  async function handleReload() {
    if (reloading) return;
    reloading = true;
    try {
      await reloadStt();
      await refreshStatus();
      // The form keeps a copy taken once, on first load. Re-read the persisted
      // row so a reload shows what the engine was actually rebuilt from rather
      // than the snapshot the page opened with.
      await reloadPersistedConfig();
      addToast($t("settings.stt.reload_toast"), "success");
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      reloading = false;
    }
  }

  $effect(() => {
    if (!liveUpdates) return;
    const id = setInterval(() => void refreshStatus(), 5000);
    return () => clearInterval(id);
  });

  $effect(() => {
    const id = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(id);
  });

  const relativeTime = $derived.by(() => {
    if (lastStatusRefresh === null) return $t("settings.stt.status_never");
    const deltaSec = Math.round((lastStatusRefresh - now) / 1000);
    try {
      const rtf = new Intl.RelativeTimeFormat($locale ?? "en", { numeric: "auto" });
      const abs = Math.abs(deltaSec);
      const [value, unit] =
        abs < 60
          ? [deltaSec, "second" as const]
          : abs < 3600
            ? [Math.round(deltaSec / 60), "minute" as const]
            : [Math.round(deltaSec / 3600), "hour" as const];
      return $t("settings.stt.status_updated", { values: { relative: rtf.format(value, unit) } });
    } catch {
      return $t("settings.stt.status_updated", { values: { relative: `${Math.abs(deltaSec)}s` } });
    }
  });

  onMount(() => {
    void settingsLoaders.sttConfig();
    void settingsLoaders.sttModels();
    void loadInputDevices();
    void refreshStatus();
  });
</script>

{#if $sttConfigStore.loading && !sttConfig}
  <SettingSectionSkeleton rows={2} />
{:else if $sttConfigStore.error}
  <ErrorBanner message={`${$t('settings.stt_error')}: ${$sttConfigStore.error}`} />
{:else if sttConfig}
  {@const cfg = sttConfig}
  <SettingsSubPage route="stt" onSave={handleSave} onReset={handleReset} data-testid="stt-config-form">
    <SttEssentialSection config={cfg} {inputDevices} {noMicrophone} />
    <SttAdvancedSection config={cfg} />
    <SttPerformanceSection config={cfg} />

    <SettingsSection
      title={$t('settings.stt.section.status_title')}
      description={$t('settings.stt.section.status_description')}
      data-testid="stt-section-status"
    >
      {#snippet icon()}<Activity size={15} strokeWidth={1.75} />{/snippet}
      {#snippet actions()}
        <div class="flex items-center gap-2">
          <Button variant="outline" size="sm" onclick={handleReload} disabled={reloading} loading={reloading} data-testid="stt-reload-btn">
            {$t('settings.stt.reload_btn')}
          </Button>
          <Button variant="ghost" size="sm" onclick={refreshStatus} data-testid="stt-status-refresh-btn">
            {#snippet icon()}<RefreshCw size={14} strokeWidth={1.75} />{/snippet}
            {$t('settings.stt.status_refresh')}
          </Button>
        </div>
      {/snippet}

      <SttTestCard silenceThresholdDb={cfg.silence_threshold_db} disabled={!cfg.enabled} />
      <div><SttStatusRows {noMicrophone} /></div>

      {#snippet footer()}
        <label class="flex cursor-pointer items-center gap-2 text-caption text-muted-foreground">
          <Checkbox bind:checked={liveUpdates} data-testid="stt-status-live-toggle" />
          {$t('settings.stt.status_live_updates')}
        </label>
        <span class="ml-auto text-caption text-muted-foreground" data-testid="stt-status-timestamp">{relativeTime}</span>
      {/snippet}
    </SettingsSection>

    {#if sttApplied}
      <div class="flex items-start gap-3 rounded-md border border-success/30 bg-success/10 px-4 py-3 text-body-sm text-success" data-testid="stt-applied-notice" role="status">
        {$t('settings.stt_applied_notice')}
      </div>
    {:else if sttNeedsRestart}
      <div class="flex items-start gap-3 rounded-md border border-warning/30 bg-warning/10 px-4 py-3 text-body-sm text-warning" data-testid="stt-restart-notice" role="alert">
        {$t('settings.stt_restart_notice')}
      </div>
    {/if}

    <div class="flex items-start gap-2.5 rounded-md border border-border/60 bg-surface-2/40 px-4 py-3 text-caption text-muted-foreground" data-testid="stt-config-hint">
      <Info size={15} strokeWidth={1.75} class="mt-px shrink-0" />
      {$t('settings.stt_config_hint')}
    </div>
  </SettingsSubPage>
{/if}
