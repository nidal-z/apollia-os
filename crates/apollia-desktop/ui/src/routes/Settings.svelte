<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { t, locale } from "svelte-i18n";
  import { navigateTo } from "$lib/stores/navigation";
  import { onboardingStore } from "$lib/stores/onboarding";
  import { themeMode, applyTheme, type ThemeMode } from "$lib/stores/theme";
  import { uiMode, type UIMode } from "$lib/stores/mode";
  import { setLocale } from "$lib/i18n";
  import { Button } from "$lib/components/ui/button";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import TabBar from "$lib/components/ui/tabs/TabBar.svelte";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { addToast } from "$lib/components/ui/toast/store";
  import UserMemories from "./settings/UserMemories.svelte";

  import type { ApollaConfigView, SystemInfo, SttModelInfo } from "$lib/types";
  import { refreshSttStatus, sttStatus } from "$lib/stores/stt";

  // ─── Types ──────────────────────────────────────────

  interface UserProfileView {
    name: string | null;
    preferences: Record<string, string>;
    habits: Record<string, string>;
    context: Record<string, string>;
  }

  interface UserMemoryEntryView {
    key: string;
    value: string;
    source: string;
    updated_at: string;
  }

  type SettingsTab = "preferences" | "profile" | "memories" | "stt" | "configuration" | "system";
  type ProfileCategory = "preferences" | "habits" | "context";

  // ─── State ──────────────────────────────────────────

  let activeTab = $state<SettingsTab>("preferences");
  let configView = $state<ApollaConfigView | null>(null);
  let systemInfo = $state<SystemInfo | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let openingEditor = $state(false);
  let resettingOnboarding = $state(false);
  let showResetConfirm = $state(false);

  // STT state
  let sttModels = $state<SttModelInfo[]>([]);
  let sttLoading = $state(false);
  let sttError = $state<string | null>(null);

  // Profile state
  let profileCategory = $state<ProfileCategory>("preferences");
  let profileName = $state("");
  let profileEntries = $state<UserMemoryEntryView[]>([]);
  let profileLoading = $state(false);
  let profileSaving = $state(false);
  let profileError = $state<string | null>(null);
  let profileUnavailable = $state(false);
  let editedValues = $state<Record<string, string>>({});
  let newEntryKey = $state("");
  let newEntryValue = $state("");
  let deleteTarget = $state<string | null>(null);
  let showDeleteConfirm = $state(false);
  let deleting = $state(false);

  const tabItems = $derived([
    { key: "preferences", label: $t("settings.preferences") },
    { key: "profile", label: $t("settings.profile") },
    { key: "memories", label: $t("settings.memories") },
    { key: "stt", label: $t("settings.stt") },
    { key: "configuration", label: $t("settings.runtime_config") },
    { key: "system", label: $t("settings.system_info") },
  ]);

  const profileCategoryItems = $derived([
    { key: "preferences", label: $t("settings.profile_tab_preferences") },
    { key: "habits", label: $t("settings.profile_tab_habits") },
    { key: "context", label: $t("settings.profile_tab_context") },
  ]);

  const SOURCE_LABELS: Record<string, string> = {
    user_explicit: "User",
    chat_inference: "Chat",
    onboarding: "Onboarding",
    agent_observation: "Agent",
  };

  // ─── Loaders ────────────────────────────────────────

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

  async function loadProfile() {
    profileLoading = true;
    profileError = null;
    profileUnavailable = false;
    try {
      const profile = await invoke<UserProfileView>("get_user_profile");
      profileName = profile.name ?? "";
      await loadMemoryEntries();
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      if (msg.includes("user memory not configured") || msg.includes("503")) {
        profileUnavailable = true;
      } else {
        profileError = msg;
      }
    } finally {
      profileLoading = false;
    }
  }

  async function loadMemoryEntries() {
    try {
      profileEntries = await invoke<UserMemoryEntryView[]>("get_user_memory", { category: profileCategory });
      editedValues = {};
      newEntryKey = "";
      newEntryValue = "";
    } catch (err: unknown) {
      profileError = err instanceof Error ? err.message : String(err);
    }
  }

  async function loadStt() {
    sttLoading = true;
    sttError = null;
    try {
      const [, models] = await Promise.all([
        refreshSttStatus(),
        invoke<SttModelInfo[]>("list_stt_models"),
      ]);
      sttModels = models;
    } catch (err: unknown) {
      sttError = err instanceof Error ? err.message : String(err);
    } finally {
      sttLoading = false;
    }
  }

  // ─── Actions ────────────────────────────────────────

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
      onboardingStore.setRequired();
      navigateTo("onboarding");
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      resettingOnboarding = false;
      showResetConfirm = false;
    }
  }

  async function saveProfile() {
    profileSaving = true;
    profileError = null;
    try {
      const updates: Record<string, string> = {};
      for (const entry of profileEntries) {
        const edited = editedValues[entry.key];
        if (edited !== undefined && edited !== entry.value) {
          updates[entry.key] = edited;
        }
      }
      if (newEntryKey.trim() && newEntryValue.trim()) {
        updates[newEntryKey.trim()] = newEntryValue.trim();
      }

      const data: Record<string, unknown> = {};
      if (profileCategory === "preferences") {
        data.name = profileName || null;
        data.preferences = Object.keys(updates).length > 0 ? updates : undefined;
      } else if (profileCategory === "habits") {
        data.habits = Object.keys(updates).length > 0 ? updates : undefined;
      } else {
        data.context = Object.keys(updates).length > 0 ? updates : undefined;
      }

      // Only save name from preferences tab
      if (profileCategory === "preferences" && profileName) {
        data.name = profileName;
      }

      await invoke("update_user_profile", { data });
      addToast($t("settings.profile_saved"), "success");
      await loadMemoryEntries();
    } catch (err: unknown) {
      profileError = err instanceof Error ? err.message : String(err);
    } finally {
      profileSaving = false;
    }
  }

  async function confirmDeleteEntry() {
    if (!deleteTarget) return;
    deleting = true;
    try {
      await invoke("forget_user_memory", { key: deleteTarget });
      await loadMemoryEntries();
    } catch (err: unknown) {
      profileError = err instanceof Error ? err.message : String(err);
    } finally {
      deleting = false;
      deleteTarget = null;
      showDeleteConfirm = false;
    }
  }

  function requestDelete(key: string) {
    deleteTarget = key;
    showDeleteConfirm = true;
  }

  async function switchProfileCategory(cat: string) {
    profileCategory = cat as ProfileCategory;
    await loadMemoryEntries();
  }

  // ─── Constants ──────────────────────────────────────

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
    ontabchange={(key) => {
      activeTab = key as SettingsTab;
      if (key === "profile") {
        loadProfile();
      } else if (key === "stt") {
        loadStt();
      }
    }}
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

    <!-- Tab: Profile -->
    {#if activeTab === "profile"}
      <section class="space-y-5" data-testid="profile-section">
        <p class="text-sm text-muted-foreground">{$t('settings.profile_subtitle')}</p>

        {#if profileUnavailable}
          <div class="rounded-md border border-warning/30 bg-warning/5 px-4 py-3 text-sm text-warning-foreground">
            {$t('settings.profile_unavailable')}
          </div>
        {:else if profileLoading}
          <div class="space-y-4">
            <Skeleton width="100%" height="3rem" />
            <Skeleton width="100%" height="12rem" />
          </div>
        {:else}
          {#if profileError}
            <div class="rounded-md border border-destructive/30 bg-destructive/5 px-4 py-2 text-sm text-destructive">
              {profileError}
            </div>
          {/if}

          <!-- Category sub-tabs -->
          <div class="inline-flex gap-1 rounded-lg glass-border p-1" data-testid="profile-category-tabs">
            {#each profileCategoryItems as cat (cat.key)}
              <button
                class="rounded-md px-3 py-1.5 text-sm font-medium transition-all duration-200
                  {profileCategory === cat.key
                    ? 'glass-surface text-foreground shadow-sm'
                    : 'bg-transparent text-muted-foreground hover:text-foreground'}"
                onclick={() => switchProfileCategory(cat.key)}
                data-testid="profile-tab-{cat.key}"
              >
                {cat.label}
              </button>
            {/each}
          </div>

          <!-- Name field (only in preferences tab) -->
          {#if profileCategory === "preferences"}
            <div class="space-y-1" data-testid="profile-name-field">
              <label class="text-sm font-medium text-muted-foreground" for="profile-name">{$t('settings.profile_name')}</label>
              <input
                id="profile-name"
                type="text"
                class="w-full max-w-md rounded-md border border-border bg-background px-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-primary/30"
                placeholder={$t('settings.profile_name_placeholder')}
                bind:value={profileName}
                data-testid="profile-name-input"
              />
            </div>
          {/if}

          <!-- Entries table -->
          <div class="glass-card glass-border rounded-lg overflow-hidden" data-testid="profile-entries-table">
            {#if profileEntries.length === 0 && !newEntryKey}
              <div class="px-4 py-8 text-center text-sm text-muted-foreground">
                {$t('settings.profile_empty')}
              </div>
            {:else}
              <table class="w-full text-sm">
                <thead>
                  <tr class="border-b border-border">
                    <th class="px-4 py-2 text-left font-medium text-muted-foreground">{$t('settings.profile_key')}</th>
                    <th class="px-4 py-2 text-left font-medium text-muted-foreground">{$t('settings.profile_value')}</th>
                    <th class="px-4 py-2 text-left font-medium text-muted-foreground">{$t('settings.profile_source')}</th>
                    <th class="w-10 px-2 py-2"></th>
                  </tr>
                </thead>
                <tbody>
                  {#each profileEntries as entry (entry.key)}
                    <tr class="border-b border-border/50 last:border-b-0 hover:bg-muted/30 transition-colors" data-testid="profile-entry-{entry.key}">
                      <td class="px-4 py-2 font-mono text-xs text-foreground">{entry.key}</td>
                      <td class="px-4 py-2">
                        <input
                          type="text"
                          class="w-full rounded border border-transparent bg-transparent px-1.5 py-0.5 text-sm text-foreground hover:border-border focus:border-primary/30 focus:outline-none focus:ring-1 focus:ring-primary/20"
                          value={editedValues[entry.key] ?? entry.value}
                          oninput={(e) => { editedValues[entry.key] = (e.target as HTMLInputElement).value; }}
                          data-testid="profile-entry-value-{entry.key}"
                        />
                      </td>
                      <td class="px-4 py-2">
                        <span class="inline-flex rounded-full px-2 py-0.5 text-xs font-medium
                          {entry.source === 'user_explicit' ? 'bg-primary/10 text-primary' :
                           entry.source === 'chat_inference' ? 'bg-info/10 text-info-foreground' :
                           entry.source === 'onboarding' ? 'bg-warning/10 text-warning-foreground' :
                           'bg-muted text-muted-foreground'}"
                          data-testid="profile-entry-source-{entry.key}"
                        >
                          {SOURCE_LABELS[entry.source] ?? entry.source}
                        </span>
                      </td>
                      <td class="px-2 py-2 text-center">
                        <button
                          class="rounded p-1 text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                          onclick={() => requestDelete(entry.key)}
                          title="Delete"
                          data-testid="profile-delete-{entry.key}"
                        >
                          <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <polyline points="3 6 5 6 21 6"></polyline>
                            <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path>
                          </svg>
                        </button>
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {/if}

            <!-- New entry row -->
            <div class="flex items-center gap-2 border-t border-border/50 px-4 py-2" data-testid="profile-new-entry">
              <input
                type="text"
                class="flex-1 rounded border border-border bg-background px-2 py-1 text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-primary/20"
                placeholder={$t('settings.profile_new_key_placeholder')}
                bind:value={newEntryKey}
                data-testid="profile-new-key"
              />
              <input
                type="text"
                class="flex-[2] rounded border border-border bg-background px-2 py-1 text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-primary/20"
                placeholder={$t('settings.profile_new_value_placeholder')}
                bind:value={newEntryValue}
                data-testid="profile-new-value"
              />
            </div>
          </div>

          <!-- Save button -->
          <div class="flex justify-end">
            <Button
              onclick={saveProfile}
              disabled={profileSaving}
              size="sm"
              data-testid="profile-save-btn"
            >
              {profileSaving ? $t('settings.profile_saving') : $t('settings.profile_save')}
            </Button>
          </div>
        {/if}
      </section>
    {/if}

    <!-- Tab: Memories (feedback loop) -->
    {#if activeTab === "memories"}
      <section data-testid="memories-section">
        <UserMemories mode={$uiMode} />
      </section>
    {/if}

    <!-- Tab: Speech-to-Text -->
    {#if activeTab === "stt"}
      <section class="space-y-5" data-testid="stt-section">
        <p class="text-sm text-muted-foreground">{$t('settings.stt_subtitle')}</p>

        {#if sttLoading}
          <div class="space-y-4">
            <Skeleton width="100%" height="8rem" />
            <Skeleton width="100%" height="8rem" />
          </div>
        {:else if sttError}
          <div class="rounded-md border border-destructive/30 bg-destructive/5 px-4 py-2 text-sm text-destructive">
            {$t('settings.stt_error')}: {sttError}
          </div>
        {:else}
          {@const status = $sttStatus}

          <!-- Engine status -->
          <div class="glass-card glass-border rounded-lg p-4" data-testid="stt-engine-status">
            <h3 class="mb-3 text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('settings.stt_engine_status')}</h3>
            <div class="space-y-2">
              <div class="grid grid-cols-2 gap-2">
                <span class="text-sm text-muted-foreground">{$t('settings.stt_engine_status')}</span>
                <span class="text-sm font-mono text-foreground">
                  {#if status?.enabled}
                    <span class="inline-flex items-center gap-1.5">
                      <span class="h-2 w-2 rounded-full bg-green-500"></span>
                      {$t('settings.stt_enabled')}
                    </span>
                  {:else}
                    <span class="inline-flex items-center gap-1.5">
                      <span class="h-2 w-2 rounded-full bg-muted-foreground"></span>
                      {$t('settings.stt_disabled')}
                    </span>
                  {/if}
                </span>
              </div>
              <div class="grid grid-cols-2 gap-2">
                <span class="text-sm text-muted-foreground">{$t('settings.stt_model_name')}</span>
                <span class="text-sm font-mono text-foreground">
                  {#if status?.model_loaded}
                    <span class="inline-flex items-center gap-1.5">
                      <span class="h-2 w-2 rounded-full bg-green-500"></span>
                      {status.model_name}
                    </span>
                  {:else}
                    <span class="text-muted-foreground">{$t('settings.stt_model_not_loaded')}</span>
                  {/if}
                </span>
              </div>
              <div class="grid grid-cols-2 gap-2">
                <span class="text-sm text-muted-foreground">{$t('settings.stt_backend')}</span>
                <span class="text-sm font-mono text-foreground">{status?.backend_name ?? "—"}</span>
              </div>
              <div class="grid grid-cols-2 gap-2">
                <span class="text-sm text-muted-foreground">{$t('settings.stt_model_path')}</span>
                <span class="text-sm font-mono text-foreground break-all">{status?.model_path ?? "—"}</span>
              </div>
              <div class="grid grid-cols-2 gap-2">
                <span class="text-sm text-muted-foreground">{$t('settings.stt_acceleration')}</span>
                <span class="text-sm font-mono text-foreground">
                  {#if status?.metal_enabled}
                    <span class="inline-flex rounded-full bg-primary/10 px-2 py-0.5 text-xs font-medium text-primary">{$t('settings.stt_metal')}</span>
                  {:else if status?.cuda_enabled}
                    <span class="inline-flex rounded-full bg-primary/10 px-2 py-0.5 text-xs font-medium text-primary">{$t('settings.stt_cuda')}</span>
                  {:else}
                    {$t('settings.stt_none')}
                  {/if}
                </span>
              </div>
            </div>
          </div>

          <!-- Configuration (from apollia.toml) -->
          {#if configView}
            {@const sttSection = configView.sections.find(s => s.name === "stt")}
            {#if sttSection}
              <div class="glass-card glass-border rounded-lg p-4" data-testid="stt-config">
                <h3 class="mb-3 text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('settings.stt_config')}</h3>
                <div class="space-y-2">
                  {#each sttSection.entries as entry (entry.key)}
                    <div class="grid grid-cols-2 gap-2">
                      <span class="text-sm text-muted-foreground">{entry.key}</span>
                      <span class="text-sm font-mono text-foreground">{entry.value}</span>
                    </div>
                  {/each}
                </div>
              </div>
            {/if}
          {/if}

          <!-- Available models -->
          <div class="glass-card glass-border rounded-lg p-4" data-testid="stt-models">
            <h3 class="mb-3 text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('settings.stt_available_models')}</h3>
            {#if sttModels.length === 0}
              <p class="text-sm text-muted-foreground">{$t('settings.stt_no_models')}</p>
            {:else}
              <div class="space-y-2">
                {#each sttModels as model (model.name)}
                  <div class="flex items-center justify-between rounded-md border border-border/50 px-3 py-2" data-testid="stt-model-{model.name}">
                    <div>
                      <span class="text-sm font-mono text-foreground">{model.name}</span>
                      {#if model.language}
                        <span class="ml-2 inline-flex rounded-full bg-primary/10 px-2 py-0.5 text-xs font-medium text-primary">{model.language}</span>
                      {/if}
                    </div>
                    <span class="text-xs text-muted-foreground">{$t('settings.stt_model_size', { values: { size: model.size_mb.toFixed(0) } })}</span>
                  </div>
                {/each}
              </div>
            {/if}
          </div>

          <!-- Hint to edit apollia.toml -->
          <div class="rounded-md border border-info/20 bg-info/5 px-4 py-3 text-sm text-info-foreground" data-testid="stt-config-hint">
            {$t('settings.stt_config_hint')}
          </div>
        {/if}
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

<!-- Delete memory entry ConfirmDialog -->
<ConfirmDialog
  open={showDeleteConfirm}
  onclose={() => { showDeleteConfirm = false; deleteTarget = null; }}
  onconfirm={confirmDeleteEntry}
  title={$t('settings.profile_delete_confirm_title')}
  message={$t('settings.profile_delete_confirm_message')}
  confirmLabel={$t('common.delete')}
  cancelLabel={$t('common.cancel')}
  loading={deleting}
  data-testid="profile-delete-confirm"
/>
