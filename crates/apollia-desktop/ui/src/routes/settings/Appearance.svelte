<script lang="ts" context="module">
  export const meta = {
    title: "settings.nav.appearance",
    icon: "palette",
    group: "settings.nav.cluster_personalization",
    cluster: "personalization",
  } as const;
</script>

<script lang="ts">
  import { t, locale } from "svelte-i18n";
  import { themeMode, applyTheme, type ThemeMode } from "$lib/stores/theme";
  import { uiMode, type UIMode } from "$lib/stores/mode";
  import { planModeDefault } from "$lib/stores/planModeSetting";
  import { setLocale } from "$lib/i18n";
  import SettingsToggle from "../../components/settings/SettingsToggle.svelte";

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

  function setPlanModeDefault(next: boolean) {
    planModeDefault.set(next);
  }
</script>

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
          <div class="absolute left-0 top-0 bottom-0 w-1 {isActive ? 'bg-primary' : 'bg-muted'}"></div>
          <div class="py-3 pl-4 pr-3">
            <span class="text-sm font-medium">{$t(option.labelKey)}</span>
            <span class="mt-0.5 block text-xs text-muted-foreground">{$t(option.descKey)}</span>
          </div>
        </button>
      {/each}
    </div>
  </div>

  <!-- Plan mode -->
  <div class="space-y-2" data-testid="plan-mode-default-section">
    <h3 class="text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('settings.planMode.title')}</h3>
    <SettingsToggle
      id="plan-mode-default"
      label={$t('settings.planMode.title')}
      description={$t('settings.planMode.description')}
      checked={$planModeDefault}
      onChange={setPlanModeDefault}
      data-testid="plan-mode-default-toggle"
    />
  </div>

</section>
