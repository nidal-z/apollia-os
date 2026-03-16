<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { currentRoute } from "$lib/stores/navigation";
  import { showOnboarding } from "$lib/stores/onboarding";
  import { themeMode, applyTheme, type ThemeMode } from "$lib/stores/theme";
  import { Button } from "$lib/components/ui/button";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { Settings as SettingsIcon } from "lucide-svelte";
  import type { ApollaConfigView } from "$lib/types";

  let configView = $state<ApollaConfigView | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let openingEditor = $state(false);
  let resettingOnboarding = $state(false);

  async function loadConfig() {
    loading = true;
    error = null;
    try {
      configView = await invoke<ApollaConfigView>("get_config");
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

  function navigateTo(route: string) {
    currentRoute.set(route as "llm" | "triggers");
  }

  const THEME_OPTIONS: { value: ThemeMode; labelKey: string }[] = [
    { value: "light", labelKey: "settings.theme_light" },
    { value: "dark", labelKey: "settings.theme_dark" },
    { value: "system", labelKey: "settings.theme_system" },
  ];

  function setTheme(mode: ThemeMode) {
    themeMode.set(mode);
    applyTheme(mode);
  }

  onMount(() => {
    loadConfig();
  });
</script>

<div class="space-y-6">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div class="flex items-center gap-3">
      <SettingsIcon size={24} class="text-muted-foreground" />
      <h1 class="text-2xl font-bold">{$t('settings.title')}</h1>
    </div>
    <Button onclick={openEditor} disabled={openingEditor}>
      {openingEditor ? $t('settings.opening') : $t('settings.open_editor')}
    </Button>
  </div>

  <!-- Info banner (AC-4) -->
  <div class="rounded-md border border-info/30 bg-info/10 px-4 py-3 text-sm text-info-foreground">
    {$t('settings.readonly_banner', { values: { file: 'apollia.toml' } })}
  </div>

  {#if error}
    <div class="rounded-md border border-[hsl(var(--destructive))] bg-[hsl(var(--destructive))]/10 px-4 py-2 text-sm text-[hsl(var(--destructive))]">
      {error}
    </div>
  {/if}

  {#if loading}
    <div class="space-y-4 py-4">
      <Skeleton width="100%" height="3rem" />
      <div class="grid gap-4 sm:grid-cols-1 lg:grid-cols-2">
        <Skeleton width="100%" height="8rem" />
        <Skeleton width="100%" height="8rem" />
      </div>
      <Skeleton width="100%" height="3rem" />
    </div>
  {:else if configView}
    <!-- Config file path -->
    <div class="text-xs text-muted-foreground">
      <span class="font-mono">{configView.config_path}</span>
      {#if !configView.config_exists}
        <span class="ml-2 rounded bg-warning/10 px-1.5 py-0.5 text-warning-foreground">{$t('settings.file_not_found')}</span>
      {/if}
    </div>

    <!-- Config sections (AC-1) -->
    <div class="grid gap-4 sm:grid-cols-1 lg:grid-cols-2">
      {#each configView.sections as section (section.name)}
        <div class="rounded-lg border bg-card p-4">
          <div class="mb-3 flex items-center justify-between">
            <h2 class="text-sm font-semibold uppercase tracking-wide text-muted-foreground">{section.name}</h2>
            <span class="text-xs text-muted-foreground">{section.description}</span>
          </div>

          {#if section.redirect_route}
            <!-- AC-2: redirect to dedicated view -->
            <button
              class="flex w-full items-center justify-between rounded-md border border-dashed px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-accent/50 hover:text-accent-foreground"
              onclick={() => navigateTo(section.redirect_route ?? "")}
            >
              <span>
                {#if section.name === "llm"}
                  {$t('settings.see_llm')}
                {:else if section.name === "triggers"}
                  {$t('settings.see_triggers')}
                {:else}
                  {$t('settings.see_details')}
                {/if}
              </span>
              <span>&rarr;</span>
            </button>
          {:else}
            <!-- Inline key/value display -->
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

    <!-- Theme section -->
    <div class="rounded-lg border bg-card p-4" data-testid="theme-section">
      <h2 class="mb-3 text-sm font-semibold uppercase tracking-wide text-muted-foreground">{$t('settings.theme_title')}</h2>
      <div class="flex items-center gap-2">
        {#each THEME_OPTIONS as option (option.value)}
          <button
            class="rounded-md border px-3 py-1.5 text-sm transition-colors {$themeMode === option.value ? 'bg-primary text-primary-foreground border-primary' : 'bg-card text-card-foreground border-border hover:bg-accent hover:text-accent-foreground'}"
            onclick={() => setTheme(option.value)}
            data-testid="theme-{option.value}"
          >
            {$t(option.labelKey)}
          </button>
        {/each}
      </div>
    </div>

    <!-- Advanced section (AC-5) -->
    <div class="rounded-lg border bg-card p-4">
      <h2 class="mb-3 text-sm font-semibold uppercase tracking-wide text-muted-foreground">{$t('settings.advanced')}</h2>
      <div class="flex items-center justify-between">
        <div>
          <p class="text-sm">{$t('settings.review_onboarding')}</p>
          <p class="text-xs text-muted-foreground">{$t('settings.review_onboarding_desc')}</p>
        </div>
        <Button variant="outline" size="sm" onclick={resetOnboarding} disabled={resettingOnboarding}>
          {resettingOnboarding ? $t('settings.resetting') : $t('settings.reset_onboarding')}
        </Button>
      </div>
    </div>
  {/if}
</div>
