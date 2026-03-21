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
  import { Settings as SettingsIcon } from "lucide-svelte";
  import type { ApollaConfigView, SystemInfo } from "$lib/types";

  type SettingsTab = "preferences" | "configuration" | "system";

  let activeTab = $state<SettingsTab>("preferences");
  let configView = $state<ApollaConfigView | null>(null);
  let systemInfo = $state<SystemInfo | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let openingEditor = $state(false);
  let resettingOnboarding = $state(false);

  const tabs: { key: SettingsTab; labelKey: string }[] = [
    { key: "preferences", labelKey: "settings.preferences" },
    { key: "configuration", labelKey: "settings.runtime_config" },
    { key: "system", labelKey: "settings.system_info" },
  ];

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

  async function resetOnboarding() {
    resettingOnboarding = true;
    try {
      await invoke("reset_onboarding");
      showOnboarding.set(true);
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      resettingOnboarding = false;
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

<div class="space-y-6">
  <!-- Header -->
  <div class="flex items-center gap-3">
    <SettingsIcon size={24} class="text-muted-foreground" />
    <div class="space-y-1">
      <h1 class="text-2xl font-semibold">{$t('settings.title')}</h1>
      <p class="text-sm text-muted-foreground" data-testid="settings-subtitle">{$t('settings.subtitle')}</p>
    </div>
  </div>

  {#if error}
    <div class="rounded-md border border-destructive/30 bg-destructive/5 px-4 py-2 text-sm text-destructive">
      {error}
    </div>
  {/if}

  <!-- Tab navigation (segmented control) -->
  <div class="inline-flex rounded-lg bg-muted p-1" data-testid="settings-tabs">
    {#each tabs as tab (tab.key)}
      <button
        class="rounded-md px-4 py-1.5 text-sm font-medium transition-all duration-150 {activeTab === tab.key
          ? 'bg-card text-foreground shadow-sm'
          : 'text-muted-foreground hover:text-foreground'}"
        onclick={() => (activeTab = tab.key)}
        data-testid="settings-tab-{tab.key}"
      >
        {$t(tab.labelKey)}
      </button>
    {/each}
  </div>

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
          <h3 class="text-sm font-medium text-muted-foreground tracking-[0.01em]">{$t('settings.language_title')}</h3>
          <div class="inline-flex rounded-lg bg-muted p-1">
            {#each LANGUAGE_OPTIONS as option (option.value)}
              <button
                class="rounded-md px-4 py-1.5 text-sm font-medium transition-all duration-150 {$locale?.startsWith(option.value)
                  ? 'bg-card text-foreground shadow-sm'
                  : 'text-muted-foreground hover:text-foreground'}"
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
          <h3 class="text-sm font-medium text-muted-foreground tracking-[0.01em]">{$t('settings.theme_title')}</h3>
          <div class="inline-flex rounded-lg bg-muted p-1" data-testid="theme-toggle">
            {#each THEME_OPTIONS as option (option.value)}
              <button
                class="rounded-md px-4 py-1.5 text-sm font-medium transition-all duration-150 {$themeMode === option.value
                  ? 'bg-card text-foreground shadow-sm'
                  : 'text-muted-foreground hover:text-foreground'}"
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
          <h3 class="text-sm font-medium text-muted-foreground tracking-[0.01em]">{$t('settings.mode_title')}</h3>
          <div class="flex gap-3" data-testid="mode-toggle">
            {#each MODE_OPTIONS as option (option.value)}
              <button
                class="flex flex-col items-start rounded-lg border px-4 py-3 text-left transition-all duration-150 {$uiMode === option.value
                  ? 'bg-primary/5 border-primary/30 text-foreground shadow-sm'
                  : 'border-border text-foreground hover:bg-muted/50'}"
                onclick={() => changeMode(option.value)}
                data-testid="mode-{option.value}"
              >
                <span class="text-sm font-medium">{$t(option.labelKey)}</span>
                <span class="text-xs text-muted-foreground">{$t(option.descKey)}</span>
              </button>
            {/each}
          </div>
        </div>

        <!-- Reset onboarding -->
        <div class="pt-2">
          <div class="flex items-center justify-between rounded-lg border border-border p-4">
            <div>
              <p class="text-sm">{$t('settings.review_onboarding')}</p>
              <p class="text-xs text-muted-foreground">{$t('settings.review_onboarding_desc')}</p>
            </div>
            <Button variant="outline" size="sm" onclick={resetOnboarding} disabled={resettingOnboarding}>
              {resettingOnboarding ? $t('settings.resetting') : $t('settings.reset_onboarding')}
            </Button>
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
              <div class="rounded-lg border border-border p-4">
                <div class="mb-3 flex items-center justify-between">
                  <h3 class="text-sm font-medium uppercase tracking-wide text-muted-foreground">{section.name}</h3>
                  <span class="text-xs text-muted-foreground">{section.description}</span>
                </div>

                {#if section.redirect_route}
                  <button
                    class="flex w-full items-center justify-between rounded-md border border-dashed border-border px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground"
                    onclick={() => navigateTo(section.redirect_route as any)}
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
                        <span class="text-sm font-mono">{entry.value}</span>
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
          <div class="rounded-lg border border-border p-4" data-testid="system-info-section">
            <h3 class="mb-3 text-sm font-medium uppercase tracking-wide text-muted-foreground">{$t('settings.system_info')}</h3>
            <div class="space-y-2">
              <div class="grid grid-cols-2 gap-2">
                <span class="text-sm text-muted-foreground">{$t('settings.system_version')}</span>
                <span class="text-sm font-mono">{systemInfo.version}</span>
              </div>
              <div class="grid grid-cols-2 gap-2">
                <span class="text-sm text-muted-foreground">{$t('settings.system_os')}</span>
                <span class="text-sm font-mono">{systemInfo.os}</span>
              </div>
              <div class="grid grid-cols-2 gap-2">
                <span class="text-sm text-muted-foreground">{$t('settings.system_python')}</span>
                <span class="text-sm font-mono">{systemInfo.python_path ?? $t('settings.system_python_not_found')}</span>
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
