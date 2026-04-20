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
  import { Accordion, AccordionItem } from "$lib/components/ui/accordion";
  import UserMemories from "./settings/UserMemories.svelte";
  import { showCommunityTemplates } from "$lib/templates/preferences";

  import type { ApollaConfigView, SystemInfo, SttModelInfo, SttConfigView, LlmBackendConfig, CliStatus } from "$lib/types";
  import { refreshSttStatus, sttStatus } from "$lib/stores/stt";

  // ─── Types ──────────────────────────────────────────

  type SettingsTab = "preferences" | "memories" | "stt" | "llm-backends" | "configuration" | "system";

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
  let sttConfig = $state<SttConfigView | null>(null);
  let sttConfigSaved = $state(false);
  let sttSaving = $state(false);
  let sttSaveError = $state<string | null>(null);

  // LLM backends state
  let llmBackendsList = $state<LlmBackendConfig[]>([]);
  let llmBackendsLoading = $state(false);
  let llmBackendsError = $state<string | null>(null);
  let llmBackendsActionError = $state<string | null>(null);
  let showAddBackendForm = $state(false);
  let addBackendSaving = $state(false);
  let newBackend = $state({
    name: "",
    provider: "llama-cpp" as LlmBackendConfig["provider"],
    model: "",
    config_json: "{}",
    enabled: true,
    is_default: false,
  });

  // CLI state
  let cliStatus = $state<CliStatus | null>(null);
  let cliActionLoading = $state(false);
  let cliError = $state<string | null>(null);

  const tabItems = $derived([
    { key: "preferences", label: $t("settings.preferences") },
    { key: "memories", label: $t("settings.profile") },
    { key: "stt", label: $t("settings.stt") },
    { key: "llm-backends", label: $t("settings.llm_backends") },
    { key: "configuration", label: $t("settings.runtime_config") },
    { key: "system", label: $t("settings.system_info") },
  ]);

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
      // Load CLI status (non-blocking — card hidden if not bundled).
      invoke<CliStatus>("get_cli_status").then(s => { cliStatus = s; }).catch(() => {});
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  async function installCli() {
    cliActionLoading = true;
    cliError = null;
    try {
      await invoke("install_cli");
      cliStatus = await invoke<CliStatus>("get_cli_status");
    } catch (err: unknown) {
      cliError = err instanceof Error ? err.message : String(err);
    } finally {
      cliActionLoading = false;
    }
  }

  async function uninstallCli() {
    cliActionLoading = true;
    cliError = null;
    try {
      await invoke("uninstall_cli");
      cliStatus = await invoke<CliStatus>("get_cli_status");
    } catch (err: unknown) {
      cliError = err instanceof Error ? err.message : String(err);
    } finally {
      cliActionLoading = false;
    }
  }

  async function loadStt() {
    sttLoading = true;
    sttError = null;
    sttConfigSaved = false;
    sttSaveError = null;
    try {
      const [config, models] = await Promise.all([
        invoke<SttConfigView>("get_stt_config"),
        invoke<SttModelInfo[]>("list_stt_models"),
      ]);
      sttConfig = { ...config };
      sttModels = models;
      // Engine status may fail if STT is not yet started — handle gracefully.
      refreshSttStatus().catch(() => {});
    } catch (err: unknown) {
      sttError = err instanceof Error ? err.message : String(err);
    } finally {
      sttLoading = false;
    }
  }

  async function saveSttConfig() {
    if (!sttConfig) return;
    sttSaving = true;
    sttSaveError = null;
    sttConfigSaved = false;
    try {
      await invoke("update_stt_config", { config: sttConfig });
      sttConfigSaved = true;
    } catch (err: unknown) {
      sttSaveError = err instanceof Error ? err.message : String(err);
    } finally {
      sttSaving = false;
    }
  }

  // ─── Hotkey capture ─────────────────────────────────
  // WebKit (Tauri/macOS) loses button focus after click, and event.ctrlKey
  // etc. are unreliable at keydown time on macOS. Solution:
  //   1. Attach listeners on document in capture phase (bypasses focus).
  //   2. Track modifier state MANUALLY via a Set on each keydown/keyup.
  //   3. preventDefault() on every key during recording so app shortcuts
  //      don't fire while the user is building the combo.
  //   4. Confirm on keyup of the non-modifier key (combo complete).

  let hotkeyRecording = $state(false);
  let hotkeyPreview  = $state('');       // live display while recording
  // Plain (non-reactive) modifier tracking — updated on every keydown/keyup.
  let _activeMods = new Set<string>();   // 'ctrl' | 'shift' | 'alt' | 'meta'

  /** Map event.key → canonical modifier label. */
  const MOD_MAP: Record<string, string> = {
    Control: 'ctrl',
    Shift:   'shift',
    Alt:     'alt',   // Option on macOS
    Meta:    'meta',  // Command on macOS
  };
  const SKIP_KEYS = new Set(['CapsLock', 'Dead', 'Unidentified', 'Process']);

  /** event.code → short key name for the hotkey string. */
  function normalizeKey(code: string, fallbackKey: string): string {
    if (code.startsWith('Key'))    return code.slice(3).toLowerCase();
    if (code.startsWith('Digit'))  return code.slice(5);
    if (code.startsWith('Numpad')) return 'numpad' + code.slice(6).toLowerCase();
    if (/^F\d+$/.test(code))       return code.toLowerCase();
    const named: Record<string, string> = {
      Space: 'space', Enter: 'enter', Backspace: 'backspace', Tab: 'tab',
      Delete: 'delete', Insert: 'insert', Escape: 'escape',
      ArrowUp: 'up', ArrowDown: 'down', ArrowLeft: 'left', ArrowRight: 'right',
      Home: 'home', End: 'end', PageUp: 'pageup', PageDown: 'pagedown',
      Minus: '-', Equal: '=', BracketLeft: '[', BracketRight: ']',
      Semicolon: ';', Quote: "'", Backquote: '`', Comma: ',', Period: '.',
      Slash: '/', Backslash: '\\', Pause: 'pause', PrintScreen: 'print',
    };
    return named[code] ?? fallbackKey.toLowerCase();
  }

  function stopHotkeyCapture() {
    hotkeyRecording = false;
    hotkeyPreview   = '';
    _activeMods.clear();
    document.removeEventListener('keydown', _onHotkeyKeydown, true);
    document.removeEventListener('keyup',   _onHotkeyKeyup,   true);
    document.removeEventListener('click',   _onHotkeyOutside,  true);
  }

  function _buildComboFromEvent(event: KeyboardEvent): string {
    // Belt-and-suspenders: use BOTH the manual Set AND event.xxxKey flags.
    // The flags are reliable at the moment a non-modifier key is pressed;
    // the Set handles macOS cases where modifier events fire after the main key.
    const parts: string[] = [];
    if (_activeMods.has('ctrl')  || event.ctrlKey)  parts.push('ctrl');
    if (_activeMods.has('shift') || event.shiftKey) parts.push('shift');
    if (_activeMods.has('alt')   || event.altKey)   parts.push('alt');
    if (_activeMods.has('meta')  || event.metaKey)  parts.push('meta');
    parts.push(normalizeKey(event.code, event.key));
    return parts.join('+');
  }

  function _onHotkeyKeydown(event: KeyboardEvent) {
    event.preventDefault();
    event.stopPropagation();
    event.stopImmediatePropagation(); // block Tauri global-shortcut handler too

    if (event.key === 'Escape') { stopHotkeyCapture(); return; }

    const mod = MOD_MAP[event.key];
    if (mod) {
      _activeMods.add(mod);
      // Show live preview of held modifiers.
      const held = [];
      if (_activeMods.has('ctrl'))  held.push('Ctrl');
      if (_activeMods.has('shift')) held.push('Shift');
      if (_activeMods.has('alt'))   held.push('Alt');
      if (_activeMods.has('meta'))  held.push('Cmd');
      hotkeyPreview = held.join('+') + (held.length ? ' +' : '');
      return;
    }
    if (SKIP_KEYS.has(event.key)) return;

    // Non-modifier key — build and immediately confirm the combo.
    const combo = _buildComboFromEvent(event);
    if (sttConfig) sttConfig.hotkey = combo;
    stopHotkeyCapture();
  }

  function _onHotkeyKeyup(event: KeyboardEvent) {
    event.preventDefault();
    event.stopPropagation();
    event.stopImmediatePropagation();

    const mod = MOD_MAP[event.key];
    if (!mod) return;

    // Capture the full set BEFORE removing this modifier.
    const parts: string[] = [];
    if (_activeMods.has('ctrl'))  parts.push('ctrl');
    if (_activeMods.has('shift')) parts.push('shift');
    if (_activeMods.has('alt'))   parts.push('alt');
    if (_activeMods.has('meta'))  parts.push('meta');

    _activeMods.delete(mod);

    if (_activeMods.size === 0) {
      // Last modifier released — confirm as modifier-only combo (e.g. ctrl+alt).
      if (parts.length > 0 && sttConfig) sttConfig.hotkey = parts.join('+');
      stopHotkeyCapture();
      return;
    }

    // Still holding some modifiers — update live preview.
    const held = [];
    if (_activeMods.has('ctrl'))  held.push('Ctrl');
    if (_activeMods.has('shift')) held.push('Shift');
    if (_activeMods.has('alt'))   held.push('Alt');
    if (_activeMods.has('meta'))  held.push('Cmd');
    hotkeyPreview = held.join('+') + ' +';
  }

  function _onHotkeyOutside(event: MouseEvent) {
    const btn = document.getElementById('stt-hotkey-btn');
    if (btn && !btn.contains(event.target as Node)) stopHotkeyCapture();
  }

  function startHotkeyCapture() {
    if (hotkeyRecording) return; // already active — avoid double-binding
    _activeMods.clear();
    hotkeyPreview  = '';
    hotkeyRecording = true;
    document.addEventListener('keydown', _onHotkeyKeydown, true);
    document.addEventListener('keyup',   _onHotkeyKeyup,   true);
    document.addEventListener('click',   _onHotkeyOutside,  true);
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

  async function loadLlmBackends() {
    llmBackendsLoading = true;
    llmBackendsError = null;
    llmBackendsActionError = null;
    try {
      llmBackendsList = await invoke<LlmBackendConfig[]>("list_llm_backends");
    } catch (err: unknown) {
      llmBackendsError = err instanceof Error ? err.message : String(err);
    } finally {
      llmBackendsLoading = false;
    }
  }

  async function handleDeleteBackend(name: string) {
    llmBackendsActionError = null;
    try {
      await invoke("delete_llm_backend", { name });
      await loadLlmBackends();
    } catch (err: unknown) {
      llmBackendsActionError = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleSetDefault(name: string) {
    llmBackendsActionError = null;
    try {
      await invoke("set_default_llm_backend", { name });
      await loadLlmBackends();
    } catch (err: unknown) {
      llmBackendsActionError = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleAddBackend() {
    llmBackendsActionError = null;
    addBackendSaving = true;
    try {
      let parsedConfig: Record<string, unknown> = {};
      try {
        parsedConfig = JSON.parse(newBackend.config_json);
      } catch {
        llmBackendsActionError = "config_json must be valid JSON";
        return;
      }
      await invoke("create_llm_backend", {
        payload: {
          name: newBackend.name,
          provider: newBackend.provider,
          model: newBackend.model,
          config_json: parsedConfig,
          enabled: newBackend.enabled,
          is_default: newBackend.is_default,
        },
      });
      showAddBackendForm = false;
      newBackend = { name: "", provider: "llama-cpp", model: "", config_json: "{}", enabled: true, is_default: false };
      await loadLlmBackends();
    } catch (err: unknown) {
      llmBackendsActionError = err instanceof Error ? err.message : String(err);
    } finally {
      addBackendSaving = false;
    }
  }

  onMount(() => {
    loadConfig();
  });
</script>

<div class="mx-auto w-full max-w-6xl space-y-6" data-testid="settings-page">
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
      if (key === "stt") {
        loadStt();
      } else if (key === "llm-backends") {
        loadLlmBackends();
      }
    }}
    testidPrefix="settings"
  />

  {#if loading}
    <div class="space-y-4 py-4">
      <Skeleton variant="table-row" />
      <Skeleton variant="card" height="8rem" />
      <Skeleton variant="card" height="8rem" />
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

        <!-- Template gallery preferences (US-SP42-058) -->
        <div class="space-y-2" data-testid="templates-preferences">
          <h3 class="text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('templates.title')}</h3>
          <label class="flex items-start justify-between gap-3 rounded-lg border border-border/60 p-3">
            <span class="flex-1">
              <span class="block text-sm text-foreground">{$t('templates.settings.community_toggle_label')}</span>
              <span class="mt-0.5 block text-xs text-muted-foreground">{$t('templates.settings.community_toggle_hint')}</span>
            </span>
            <input
              type="checkbox"
              class="mt-1 h-4 w-4 accent-primary"
              bind:checked={$showCommunityTemplates}
              data-testid="templates-community-toggle"
            />
          </label>
        </div>

        <!-- Advanced (collapsible) -->
        <div class="pt-2 glass-card glass-border rounded-lg">
          <Accordion type="single" collapsible>
            <AccordionItem value="advanced">
              {#snippet trigger()}
                <span>{$t('settings.advanced')}</span>
              {/snippet}
              {#snippet content()}
                <div class="flex items-center justify-between pt-1">
                  <div>
                    <p class="text-sm text-foreground">{$t('settings.review_onboarding')}</p>
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
              {/snippet}
            </AccordionItem>
          </Accordion>
        </div>
      </section>
    {/if}

    <!-- Tab: Profile (unified — replaces legacy profile + memories) -->
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

          <!-- ── Configuration (editable) ──────────────────────────── -->
          {#if sttConfig}
            <div class="glass-card glass-border rounded-lg p-4" data-testid="stt-config-form">
              <h3 class="mb-4 text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('settings.stt_config_section')}</h3>

              <div class="space-y-4">
                <!-- Enable toggle -->
                <label class="flex cursor-pointer items-center justify-between" data-testid="stt-enable-toggle">
                  <span class="text-sm text-foreground">{$t('settings.stt_enable_engine')}</span>
                  <button
                    type="button"
                    role="switch"
                    aria-checked={sttConfig.enabled}
                    aria-label={$t('settings.stt_enable_engine')}
                    onclick={() => { if (sttConfig) sttConfig.enabled = !sttConfig.enabled; }}
                    class="relative inline-flex h-6 w-11 items-center rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring {sttConfig.enabled ? 'bg-primary' : 'bg-muted'}"
                  >
                    <span class="inline-block h-4 w-4 rounded-full bg-white shadow-sm transition-transform {sttConfig.enabled ? 'translate-x-6' : 'translate-x-1'}"></span>
                  </button>
                </label>

                <!-- Model selector -->
                <div class="space-y-1.5">
                  <label class="text-sm text-muted-foreground" for="stt-model-select">{$t('settings.stt_select_model')}</label>
                  {#if sttModels.length > 0}
                    <select
                      id="stt-model-select"
                      bind:value={sttConfig.model_path}
                      class="flex h-9 w-full appearance-none rounded-md border border-border bg-background px-3 py-1.5 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
                      data-testid="stt-model-select"
                    >
                      {#each sttModels as model (model.name)}
                        <option value="~/.apollia/models/{model.name}">
                          {model.name} ({model.size_mb.toFixed(0)} Mo{model.language ? ` · ${model.language}` : ''})
                        </option>
                      {/each}
                    </select>
                  {:else}
                    <p class="rounded-md border border-border/50 px-3 py-2 text-sm text-muted-foreground">
                      {$t('settings.stt_no_models')}
                    </p>
                  {/if}
                </div>

                <!-- Language -->
                <div class="space-y-1.5">
                  <label class="text-sm text-muted-foreground" for="stt-language">{$t('settings.stt_language')}</label>
                  <input
                    id="stt-language"
                    type="text"
                    placeholder={$t('settings.stt_language_auto')}
                    bind:value={sttConfig.language}
                    class="flex h-9 w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
                    data-testid="stt-language-input"
                  />
                </div>

                <!-- Hotkey capture -->
                <div class="space-y-1.5">
                  <label class="text-sm text-muted-foreground" for="stt-hotkey-btn">{$t('settings.stt_hotkey')}</label>
                  <button
                    id="stt-hotkey-btn"
                    type="button"
                    aria-label={hotkeyRecording ? $t('settings.stt_hotkey_recording') : sttConfig.hotkey || $t('settings.stt_hotkey_placeholder')}
                    class="flex h-9 w-full cursor-pointer items-center justify-between rounded-md border px-3 py-1.5 text-sm transition-colors {hotkeyRecording ? 'border-primary bg-primary/5 text-primary ring-2 ring-primary/30' : 'border-border bg-background text-foreground hover:border-border/80'}"
                    data-testid="stt-hotkey-input"
                    onclick={startHotkeyCapture}
                  >
                    <span class="font-mono">
                      {#if hotkeyRecording}
                        {#if hotkeyPreview}
                          <span>{hotkeyPreview}</span>
                        {:else}
                          <span class="animate-pulse">{$t('settings.stt_hotkey_recording')}</span>
                        {/if}
                      {:else}
                        {sttConfig.hotkey || $t('settings.stt_hotkey_placeholder')}
                      {/if}
                    </span>
                    {#if !hotkeyRecording}
                      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor" class="h-3.5 w-3.5 text-muted-foreground" aria-hidden="true">
                        <path d="M13.488 2.513a1.75 1.75 0 0 0-2.475 0L6.75 6.774a2.75 2.75 0 0 0-.596.892l-.848 2.047a.75.75 0 0 0 .98.98l2.047-.848a2.75 2.75 0 0 0 .892-.596l4.261-4.262a1.75 1.75 0 0 0 0-2.474Z" />
                        <path d="M4.75 3.5c-.69 0-1.25.56-1.25 1.25v6.5c0 .69.56 1.25 1.25 1.25h6.5c.69 0 1.25-.56 1.25-1.25V8a.75.75 0 0 1 1.5 0v3.25A2.75 2.75 0 0 1 11.25 14h-6.5A2.75 2.75 0 0 1 2 11.25v-6.5A2.75 2.75 0 0 1 4.75 2H8a.75.75 0 0 1 0 1.5H4.75Z" />
                      </svg>
                    {/if}
                  </button>
                  {#if hotkeyRecording}
                    <p class="text-xs text-primary/80">{$t('settings.stt_hotkey_hint')}</p>
                  {/if}
                </div>

                <!-- Trigger mode + Clipboard mode (2-col grid) -->
                <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                  <div class="space-y-1.5">
                    <label class="text-sm text-muted-foreground" for="stt-trigger">{$t('settings.stt_trigger_mode')}</label>
                    <select
                      id="stt-trigger"
                      bind:value={sttConfig.trigger_mode}
                      class="flex h-9 w-full appearance-none rounded-md border border-border bg-background px-3 py-1.5 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
                    >
                      <option value="toggle">{$t('settings.stt_trigger_toggle')}</option>
                      <option value="push-to-talk">{$t('settings.stt_trigger_push')}</option>
                    </select>
                  </div>

                  <div class="space-y-1.5">
                    <label class="text-sm text-muted-foreground" for="stt-clipboard">{$t('settings.stt_clipboard_mode')}</label>
                    <select
                      id="stt-clipboard"
                      bind:value={sttConfig.clipboard_mode}
                      class="flex h-9 w-full appearance-none rounded-md border border-border bg-background px-3 py-1.5 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
                    >
                      <option value="paste">{$t('settings.stt_clipboard_paste')}</option>
                      <option value="clipboard">{$t('settings.stt_clipboard_clipboard')}</option>
                    </select>
                  </div>
                </div>

                <!-- Max recording + Silence threshold (2-col grid) -->
                <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                  <div class="space-y-1.5">
                    <label class="text-sm text-muted-foreground" for="stt-max-rec">{$t('settings.stt_max_recording')}</label>
                    <div class="relative">
                      <input
                        id="stt-max-rec"
                        type="number"
                        min="5"
                        max="300"
                        bind:value={sttConfig.max_recording_sec}
                        class="flex h-9 w-full rounded-md border border-border bg-background px-3 py-1.5 pr-8 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
                      />
                      <span class="pointer-events-none absolute inset-y-0 right-2.5 flex items-center text-xs text-muted-foreground">s</span>
                    </div>
                  </div>

                  <div class="space-y-1.5">
                    <label class="text-sm text-muted-foreground" for="stt-silence">{$t('settings.stt_silence_threshold')}</label>
                    <div class="relative">
                      <input
                        id="stt-silence"
                        type="number"
                        min="-80"
                        max="0"
                        step="1"
                        bind:value={sttConfig.silence_threshold_db}
                        class="flex h-9 w-full rounded-md border border-border bg-background px-3 py-1.5 pr-10 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
                      />
                      <span class="pointer-events-none absolute inset-y-0 right-2.5 flex items-center text-xs text-muted-foreground">dB</span>
                    </div>
                  </div>
                </div>

                <!-- Clipboard restore toggle -->
                <label class="flex cursor-pointer items-center justify-between">
                  <span class="text-sm text-muted-foreground">{$t('settings.stt_clipboard_restore')}</span>
                  <button
                    type="button"
                    role="switch"
                    aria-checked={sttConfig.clipboard_restore}
                    aria-label={$t('settings.stt_clipboard_restore')}
                    onclick={() => { if (sttConfig) sttConfig.clipboard_restore = !sttConfig.clipboard_restore; }}
                    class="relative inline-flex h-6 w-11 items-center rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring {sttConfig.clipboard_restore ? 'bg-primary' : 'bg-muted'}"
                  >
                    <span class="inline-block h-4 w-4 rounded-full bg-white shadow-sm transition-transform {sttConfig.clipboard_restore ? 'translate-x-6' : 'translate-x-1'}"></span>
                  </button>
                </label>
              </div>

              <!-- Save button + errors -->
              <div class="mt-5 flex items-center gap-3">
                <Button
                  onclick={saveSttConfig}
                  disabled={sttSaving}
                  data-testid="stt-save-btn"
                >
                  {sttSaving ? $t('settings.stt_saving') : $t('settings.stt_save')}
                </Button>
                {#if sttSaveError}
                  <span class="text-sm text-destructive">{$t('settings.stt_save_error')}: {sttSaveError}</span>
                {/if}
              </div>
            </div>
          {/if}

          <!-- ── Restart notice ─────────────────────────────────────── -->
          {#if sttConfigSaved}
            <div class="flex items-start gap-3 rounded-md border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm text-amber-700 dark:text-amber-400" data-testid="stt-restart-notice" role="alert">
              <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="mt-0.5 h-4 w-4 shrink-0">
                <path fill-rule="evenodd" d="M8.485 2.495c.673-1.167 2.357-1.167 3.03 0l6.28 10.875c.673 1.167-.17 2.625-1.516 2.625H3.72c-1.347 0-2.189-1.458-1.515-2.625L8.485 2.495zM10 5a.75.75 0 01.75.75v3.5a.75.75 0 01-1.5 0v-3.5A.75.75 0 0110 5zm0 9a1 1 0 100-2 1 1 0 000 2z" clip-rule="evenodd" />
              </svg>
              {$t('settings.stt_restart_notice')}
            </div>
          {/if}

          <!-- ── Engine runtime status (read-only) ─────────────────── -->
          {@const status = $sttStatus}
          <div class="glass-card glass-border rounded-lg p-4" data-testid="stt-engine-status">
            <h3 class="mb-3 text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('settings.stt_engine_status_section')}</h3>
            <div class="space-y-2">
              <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
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
              <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
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
              <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
                <span class="text-sm text-muted-foreground">{$t('settings.stt_backend')}</span>
                <span class="text-sm font-mono text-foreground">{status?.backend_name ?? "—"}</span>
              </div>
              <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
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

          <!-- ── Available models ──────────────────────────────────── -->
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

          <!-- ── Config file hint ──────────────────────────────────── -->
          <div class="rounded-md border border-border/30 bg-muted/20 px-4 py-3 text-xs text-muted-foreground" data-testid="stt-config-hint">
            {$t('settings.stt_config_hint')}
          </div>
        {/if}
      </section>
    {/if}

    <!-- Tab: LLM Backends -->
    {#if activeTab === "llm-backends"}
      <section class="space-y-4" data-testid="llm-backends-section">
        <div class="flex items-center justify-between">
          <p class="text-sm text-muted-foreground">{$t('settings.llm_backends_subtitle')}</p>
          <Button
            size="sm"
            onclick={() => { showAddBackendForm = !showAddBackendForm; llmBackendsActionError = null; }}
            data-testid="add-backend-btn"
          >
            {showAddBackendForm ? $t('common.cancel') : $t('settings.llm_add_backend')}
          </Button>
        </div>

        {#if llmBackendsActionError}
          <div class="rounded-md border border-destructive/30 bg-destructive/5 px-4 py-2 text-sm text-destructive">
            {llmBackendsActionError}
          </div>
        {/if}

        <!-- Add backend inline form -->
        {#if showAddBackendForm}
          <div class="glass-card glass-border rounded-lg p-4 space-y-4" data-testid="add-backend-form">
            <h3 class="text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('settings.llm_new_backend')}</h3>

            <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
              <div class="space-y-1.5">
                <label class="text-sm text-muted-foreground" for="backend-name">{$t('settings.llm_backend_name')}</label>
                <input
                  id="backend-name"
                  type="text"
                  placeholder="local-code"
                  bind:value={newBackend.name}
                  class="flex h-9 w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm font-mono focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
                  data-testid="backend-name-input"
                />
              </div>

              <div class="space-y-1.5">
                <label class="text-sm text-muted-foreground" for="backend-provider">{$t('settings.llm_backend_provider')}</label>
                <select
                  id="backend-provider"
                  bind:value={newBackend.provider}
                  class="flex h-9 w-full appearance-none rounded-md border border-border bg-background px-3 py-1.5 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
                  data-testid="backend-provider-select"
                >
                  <option value="llama-cpp">llama-cpp (local)</option>
                  <option value="openai">OpenAI</option>
                  <option value="mistral">Mistral</option>
                  <option value="anthropic">Anthropic</option>
                  <option value="ollama">Ollama</option>
                </select>
              </div>
            </div>

            <div class="space-y-1.5">
              <label class="text-sm text-muted-foreground" for="backend-model">{$t('settings.llm_backend_model')}</label>
              <input
                id="backend-model"
                type="text"
                placeholder="qwen3-0.6b-q8_0"
                bind:value={newBackend.model}
                class="flex h-9 w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm font-mono focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
                data-testid="backend-model-input"
              />
            </div>

            <div class="space-y-1.5">
              <label class="text-sm text-muted-foreground" for="backend-config">{$t('settings.llm_backend_config_json')}</label>
              <textarea
                id="backend-config"
                rows={3}
                bind:value={newBackend.config_json}
                class="flex w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm font-mono focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
                data-testid="backend-config-input"
              ></textarea>
            </div>

            <div class="flex items-center gap-6">
              <label class="flex cursor-pointer items-center gap-2">
                <button
                  type="button"
                  role="switch"
                  aria-checked={newBackend.enabled}
                  aria-label={$t('settings.llm_backend_enabled')}
                  onclick={() => { newBackend.enabled = !newBackend.enabled; }}
                  class="relative inline-flex h-5 w-9 items-center rounded-full transition-colors {newBackend.enabled ? 'bg-primary' : 'bg-muted'}"
                >
                  <span class="inline-block h-3.5 w-3.5 rounded-full bg-white shadow-sm transition-transform {newBackend.enabled ? 'translate-x-4.5' : 'translate-x-0.5'}"></span>
                </button>
                <span class="text-sm text-muted-foreground">{$t('settings.llm_backend_enabled')}</span>
              </label>

              <label class="flex cursor-pointer items-center gap-2">
                <button
                  type="button"
                  role="switch"
                  aria-checked={newBackend.is_default}
                  aria-label={$t('settings.llm_backend_default')}
                  onclick={() => { newBackend.is_default = !newBackend.is_default; }}
                  class="relative inline-flex h-5 w-9 items-center rounded-full transition-colors {newBackend.is_default ? 'bg-primary' : 'bg-muted'}"
                >
                  <span class="inline-block h-3.5 w-3.5 rounded-full bg-white shadow-sm transition-transform {newBackend.is_default ? 'translate-x-4.5' : 'translate-x-0.5'}"></span>
                </button>
                <span class="text-sm text-muted-foreground">{$t('settings.llm_backend_default')}</span>
              </label>
            </div>

            <Button
              onclick={handleAddBackend}
              disabled={addBackendSaving || !newBackend.name || !newBackend.model}
              data-testid="add-backend-save-btn"
            >
              {addBackendSaving ? $t('common.saving') : $t('settings.llm_add_backend')}
            </Button>
          </div>
        {/if}

        <!-- Backends list -->
        {#if llmBackendsLoading}
          <div class="space-y-2">
            {#each { length: 2 } as _}
              <Skeleton width="100%" height="3rem" />
            {/each}
          </div>
        {:else if llmBackendsError}
          <div class="rounded-md border border-destructive/30 bg-destructive/5 px-4 py-2 text-sm text-destructive">
            {llmBackendsError}
          </div>
        {:else if llmBackendsList.length === 0}
          <div class="rounded-lg border border-dashed border-border px-4 py-8 text-center text-sm text-muted-foreground" data-testid="llm-backends-empty">
            {$t('settings.llm_no_backends')}
          </div>
        {:else}
          <div class="space-y-2" data-testid="llm-backends-list">
            {#each llmBackendsList as backend (backend.name)}
              <div class="glass-card glass-border flex items-center justify-between rounded-lg px-4 py-3" data-testid="llm-backend-row-{backend.name}">
                <div class="flex items-center gap-3">
                  <div>
                    <div class="flex items-center gap-2">
                      <span class="text-sm font-mono font-medium">{backend.name}</span>
                      {#if backend.is_default}
                        <span class="inline-flex rounded-full bg-primary/10 px-2 py-0.5 text-xs font-medium text-primary">default</span>
                      {/if}
                      {#if !backend.enabled}
                        <span class="inline-flex rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground">disabled</span>
                      {/if}
                    </div>
                    <p class="text-xs text-muted-foreground">{backend.provider} · {backend.model}</p>
                  </div>
                </div>
                <div class="flex items-center gap-2">
                  {#if !backend.is_default}
                    <Button
                      size="sm"
                      variant="outline"
                      onclick={() => handleSetDefault(backend.name)}
                      data-testid="set-default-{backend.name}"
                    >
                      {$t('settings.llm_set_default')}
                    </Button>
                    <Button
                      size="sm"
                      variant="destructive"
                      onclick={() => handleDeleteBackend(backend.name)}
                      data-testid="delete-backend-{backend.name}"
                    >
                      {$t('common.delete')}
                    </Button>
                  {/if}
                </div>
              </div>
            {/each}
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
                      <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
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
              <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
                <span class="text-sm text-muted-foreground">{$t('settings.system_version')}</span>
                <span class="text-sm font-mono text-foreground">{systemInfo.version}</span>
              </div>
              <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
                <span class="text-sm text-muted-foreground">{$t('settings.system_os')}</span>
                <span class="text-sm font-mono text-foreground">{systemInfo.os}</span>
              </div>
              <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
                <span class="text-sm text-muted-foreground">{$t('settings.system_python')}</span>
                <span class="text-sm font-mono text-foreground">{systemInfo.python_path ?? $t('settings.system_python_not_found')}</span>
              </div>
            </div>
          </div>
        {:else}
          <p class="text-sm text-muted-foreground">{$t('common.loading')}</p>
        {/if}

        {#if cliStatus?.bundled}
          <div class="glass-card glass-border rounded-lg p-4">
            <h3 class="mb-3 text-sm font-medium uppercase tracking-wider text-muted-foreground">
              {$t('settings.cli_title')}
            </h3>
            <p class="text-sm text-muted-foreground mb-3">
              {$t('settings.cli_description')}
            </p>
            <div class="grid grid-cols-1 sm:grid-cols-2 gap-2 mb-3">
              <span class="text-sm text-muted-foreground">{$t('settings.cli_status')}</span>
              <span class="text-sm font-mono text-foreground">
                {cliStatus.installed ? $t('settings.cli_installed') : $t('settings.cli_not_installed')}
              </span>
              <span class="text-sm text-muted-foreground">{$t('settings.cli_path')}</span>
              <span class="text-sm font-mono text-foreground">{cliStatus.symlink_path}</span>
            </div>
            {#if cliStatus.installed}
              <Button variant="outline" size="sm" onclick={uninstallCli} disabled={cliActionLoading}>
                {cliActionLoading ? $t('common.loading') : $t('settings.cli_uninstall')}
              </Button>
            {:else}
              <Button variant="default" size="sm" onclick={installCli} disabled={cliActionLoading}>
                {cliActionLoading ? $t('common.loading') : $t('settings.cli_install')}
              </Button>
              {#if cliStatus.needs_privilege}
                <p class="text-xs text-muted-foreground mt-1">{$t('settings.cli_needs_privilege')}</p>
              {/if}
            {/if}
            {#if cliError}
              <p class="text-sm text-destructive mt-2">{cliError}</p>
            {/if}
          </div>
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

