<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { t, locale } from "svelte-i18n";
  import { navigateTo } from "$lib/stores/navigation";
  import { showOnboarding } from "$lib/stores/onboarding";
  import { themeMode, applyTheme, type ThemeMode } from "$lib/stores/theme";
  import { uiMode, type UIMode } from "$lib/stores/mode";
  import { setLocale } from "$lib/i18n";
  import { Button } from "$lib/components/ui/button";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import TabBar from "$lib/components/ui/tabs/TabBar.svelte";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";

  import type { ApollaConfigView, SystemInfo } from "$lib/types";

  type SettingsTab = "preferences" | "configuration" | "system";

  let activeTab = $state<SettingsTab>("preferences");
  let configView = $state<ApollaConfigView | null>(null);
  let systemInfo = $state<SystemInfo | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let openingEditor = $state(false);
  let resettingOnboarding = $state(false);
  let showResetConfirm = $state(false);

  const tabItems = $derived([
    { key: "preferences", label: $t("settings.preferences") },
    { key: "configuration", label: $t("settings.runtime_config") },
    { key: "system", label: $t("settings.system_info") },
  ]);

  async function loadConfig() {
    loading = true;
    error = null;
    try {
      const [config, info] = await Promise.all([
        invoke<ApollaConfigView>("get_config"),
        invoke<SystemInfo>("get_system_info"),
      ]);
      configView = config;
      systemInfo = info;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  async function openEditor() {
    openingEditor = true;
    try {
      await invoke("open_config_in_editor");
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      openingEditor = false;
    }
  }

  async function confirmResetOnboarding() {
    resettingOnboarding = true;
    try {
      await invoke("reset_onboarding");
      showOnboarding.set(true);
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      resettingOnboarding = false;
      showResetConfirm = false;
    }
  }

  const THEME_OPTIONS: { value: ThemeMode; labelKey: string }[] = [
    { value: "light", labelKey: "settings.theme_light" },
    { value: "dark", labelKey: "settings.theme_dark" },
    { value: "system", labelKey: "settings.theme_system" },
  ];

  const LANGUAGE_OPTIONS: { value: string; labelKey: string }[] = [
    { value: "en", labelKey: "settings.language_en" },
    { value: "fr", labelKey: "settings.language_fr" },
  ];

  const MODE_OPTIONS: { value: UIMode; labelKey: string; descKey: string }[] = [
    { value: "operator", labelKey: "settings.mode_operator", descKey: "settings.mode_operator_desc" },
    { value: "builder", labelKey: "settings.mode_builder", descKey: "settings.mode_builder_desc" },
  ];

  function setTheme(mode: ThemeMode) {
    themeMode.set(mode);
    applyTheme(mode);
  }

  function changeLocale(lang: string) {
    locale.set(lang);
    setLocale(lang);
  }

  function changeMode(mode: UIMode) {
    uiMode.set(mode);
  }

  onMount(() => {
    loadConfig();
  });
</script>

<div class="max-w-6xl space-y-6" data-testid="settings-page">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-semibold">{$t('settings.title')}</h1>
      <p class="text-xs text-muted-foreground" data-testid="settings-subtitle">{$t('settings.subtitle')}</p>
    </div>
  </div>

  {#if error}
    <div class="rounded-md border border-destructive/30 bg-destructive/5 px-4 py-2 text-sm text-destructive">
      {error}
    </div>
  {/if}

  <!-- TabBar component -->
  <TabBar
    items={tabItems}
    activeTab={activeTab}
    ontabchange={(key) => { activeTab = key as SettingsTab; }}
    testidPrefix="settings"
  />

  {#if loading}
    <div class="space-y-4 py-4">
      <Skeleton width="100%" height="3rem" />
      <Skeleton width="100%" height="8rem" />
      <Skeleton width="100%" height="8rem" />
    </div>
  {:else}
    <!-- Tab: Preferences -->
    {#if activeTab === "preferences"}
      <section class="space-y-5" data-testid="preferences-section">
        <!-- Language -->
        <div class="space-y-2">
          <h3 class="text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('settings.language_title')}</h3>
          <div class="inline-flex gap-1 rounded-lg glass-border p-1">
            {#each LANGUAGE_OPTIONS as option (option.value)}
              <button
                class="rounded-md px-3 py-1.5 text-sm font-medium transition-all duration-200
                  {$locale?.startsWith(option.value)
                    ? 'glass-surface text-foreground shadow-sm'
                    : 'bg-transparent text-muted-foreground hover:text-foreground'}"
                onclick={() => changeLocale(option.value)}
                data-testid="language-{option.value}"
              >
                {$t(option.labelKey)}
              </button>
            {/each}
          </div>
        </div>

        <!-- Theme -->
        <div class="space-y-2">
          <h3 class="text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('settings.theme_title')}</h3>
          <div class="inline-flex gap-1 rounded-lg glass-border p-1" data-testid="theme-toggle">
            {#each THEME_OPTIONS as option (option.value)}
              <button
                class="rounded-md px-3 py-1.5 text-sm font-medium transition-all duration-200
                  {$themeMode === option.value
                    ? 'glass-surface text-foreground shadow-sm'
                    : 'bg-transparent text-muted-foreground hover:text-foreground'}"
                onclick={() => setTheme(option.value)}
                data-testid="theme-{option.value}"
              >
                {$t(option.labelKey)}
              </button>
            {/each}
          </div>
        </div>

        <!-- Mode -->
        <div class="space-y-2">
          <h3 class="text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('settings.mode_title')}</h3>
          <div class="flex gap-3" data-testid="mode-toggle">
            {#each MODE_OPTIONS as option (option.value)}
              {@const isActive = $uiMode === option.value}
              <button
                class="glass-card-hover relative flex flex-1 items-start overflow-hidden rounded-lg text-left transition-all duration-200
                  {isActive ? 'ring-1 ring-primary/20' : ''}"
                onclick={() => changeMode(option.value)}
                data-testid="mode-{option.value}"
              >
                <!-- Accent bar left -->
                <div class="absolute left-0 top-0 bottom-0 w-1 {isActive ? 'bg-primary' : 'bg-muted'}" />
                <div class="py-3 pl-4 pr-3">
                  <span class="text-sm font-medium">{$t(option.labelKey)}</span>
                  <span class="mt-0.5 block text-xs text-muted-foreground">{$t(option.descKey)}</span>
                </div>
              </button>
            {/each}
          </div>
        </div>

        <!-- Reset onboarding with ConfirmDialog -->
        <div class="pt-2">
          <div class="glass-card glass-border rounded-lg p-4">
            <div class="flex items-center justify-between">
              <div>
                <p class="text-sm">{$t('settings.review_onboarding')}</p>
                <p class="text-xs text-muted-foreground">{$t('settings.review_onboarding_desc')}</p>
              </div>
              <Button
                variant="outline"
                size="sm"
                onclick={() => { showResetConfirm = true; }}
                data-testid="reset-onboarding-btn"
              >
                {$t('settings.reset_onboarding')}
              </Button>
            </div>
          </div>
        </div>
      </section>
    {/if}

    <!-- Tab: Configuration -->
    {#if activeTab === "configuration"}
      <section class="space-y-4" data-testid="runtime-config-section">
        <div class="flex items-center justify-between">
          <p class="text-sm text-muted-foreground">{$t('settings.readonly_banner', { values: { file: 'apollia.toml' } })}</p>
          <Button onclick={openEditor} disabled={openingEditor} variant="outline" size="sm">
            {openingEditor ? $t('settings.opening') : $t('settings.open_editor')}
          </Button>
        </div>

        <div class="rounded-md border border-info/20 bg-info/5 px-4 py-3 text-sm text-info-foreground" data-testid="settings-operational-redirect-banner">
          {$t('settings.operational_redirect_banner')}
        </div>

        {#if configView}
          <div class="text-xs text-muted-foreground">
            <span class="font-mono">{configView.config_path}</span>
            {#if !configView.config_exists}
              <span class="ml-2 rounded bg-warning/10 px-1.5 py-0.5 text-warning-foreground">{$t('settings.file_not_found')}</span>
            {/if}
          </div>

          <div class="grid gap-4 sm:grid-cols-1 lg:grid-cols-2">
            {#each configView.sections as section (section.name)}
              <div class="glass-card glass-border rounded-lg p-4">
                <div class="mb-3 flex items-center justify-between">
                  <h3 class="text-sm font-medium uppercase tracking-wider text-muted-foreground">{section.name}</h3>
                  <span class="text-xs text-muted-foreground">{section.description}</span>
                </div>

                {#if section.redirect_route}
                  <button
                    class="flex w-full items-center justify-between rounded-md border border-dashed border-border px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                    onclick={() => navigateTo(section.redirect_route as any)}
                    data-testid="settings-redirect-{section.name}"
                  >
                    <span>
                      {#if section.name === "llm"}
                        {$t('settings.see_llm')}
                      {:else}
                        {$t('settings.see_details')}
                      {/if}
                    </span>
                    <span>&rarr;</span>
                  </button>
                {:else}
                  <div class="space-y-2">
                    {#each section.entries as entry (entry.key)}
                      <div class="grid grid-cols-2 gap-2">
                        <span class="text-sm text-muted-foreground">{entry.key}</span>
                        <span class="text-sm font-mono text-foreground">{entry.value}</span>
                      </div>
                    {/each}
                  </div>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </section>
    {/if}

    <!-- Tab: System -->
    {#if activeTab === "system"}
      <section class="space-y-4" data-testid="advanced-section">
        {#if systemInfo}
          <div class="glass-card glass-border rounded-lg p-4" data-testid="system-info-section">
            <h3 class="mb-3 text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('settings.system_info')}</h3>
            <div class="space-y-2">
              <div class="grid grid-cols-2 gap-2">
                <span class="text-sm text-muted-foreground">{$t('settings.system_version')}</span>
                <span class="text-sm font-mono text-foreground">{systemInfo.version}</span>
              </div>
              <div class="grid grid-cols-2 gap-2">
                <span class="text-sm text-muted-foreground">{$t('settings.system_os')}</span>
                <span class="text-sm font-mono text-foreground">{systemInfo.os}</span>
              </div>
              <div class="grid grid-cols-2 gap-2">
                <span class="text-sm text-muted-foreground">{$t('settings.system_python')}</span>
                <span class="text-sm font-mono text-foreground">{systemInfo.python_path ?? $t('settings.system_python_not_found')}</span>
              </div>
            </div>
          </div>
        {:else}
          <p class="text-sm text-muted-foreground">{$t('common.loading')}</p>
        {/if}
      </section>
    {/if}
  {/if}
</div>

<!-- Reset onboarding ConfirmDialog -->
<ConfirmDialog
  open={showResetConfirm}
  onclose={() => { showResetConfirm = false; }}
  onconfirm={confirmResetOnboarding}
  title={$t('settings.reset_confirm_title')}
  message={$t('settings.reset_confirm_message')}
  confirmLabel={$t('settings.reset_onboarding')}
  cancelLabel={$t('common.cancel')}
  loading={resettingOnboarding}
  data-testid="reset-onboarding-confirm"
/>
