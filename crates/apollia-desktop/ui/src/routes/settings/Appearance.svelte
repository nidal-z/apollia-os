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
  import {
    Globe,
    Palette,
    Sun,
    Moon,
    Monitor,
    Eye,
    SlidersHorizontal,
    Check,
    User,
    Wrench,
  } from "lucide-svelte";
  import { themeMode, applyTheme, type ThemeMode } from "$lib/stores/theme";
  import { uiMode, type UIMode } from "$lib/stores/mode";
  import { planModeDefault } from "$lib/stores/planModeSetting";
  import { setLocale } from "$lib/i18n";
  import { Toggle } from "$lib/components/ui/toggle";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import SettingsSection from "../../components/settings/SettingsSection.svelte";
  import SettingsFieldRow from "../../components/settings/SettingsFieldRow.svelte";

  type Glyph = typeof Sun;
  type SegOption = { value: string; labelKey: string; icon?: Glyph };
  type ModeOption = { value: UIMode; labelKey: string; descKey: string; icon: Glyph };

  const LANGUAGE_OPTIONS: SegOption[] = [
    { value: "fr", labelKey: "settings.language_fr" },
    { value: "en", labelKey: "settings.language_en" },
  ];

  const THEME_OPTIONS: SegOption[] = [
    { value: "light", labelKey: "settings.theme_light", icon: Sun },
    { value: "dark", labelKey: "settings.theme_dark", icon: Moon },
    { value: "system", labelKey: "settings.theme_system", icon: Monitor },
  ];

  const MODE_OPTIONS: ModeOption[] = [
    { value: "operator", labelKey: "settings.mode_operator", descKey: "settings.mode_operator_desc", icon: User },
    { value: "builder", labelKey: "settings.mode_builder", descKey: "settings.mode_builder_desc", icon: Wrench },
  ];

  function selectTheme(value: string) {
    const mode = value as ThemeMode;
    themeMode.set(mode);
    applyTheme(mode);
  }

  function selectLocale(value: string) {
    locale.set(value);
    setLocale(value);
  }

  function selectMode(value: string) {
    uiMode.set(value as UIMode);
  }

  const currentLang = $derived($locale?.startsWith("fr") ? "fr" : "en");

  let systemDark = $state(false);
  $effect(() => {
    const mq = globalThis.matchMedia("(prefers-color-scheme: dark)");
    systemDark = mq.matches;
    const onChange = (e: MediaQueryListEvent) => (systemDark = e.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  });

  const lightActive = $derived(
    $themeMode === "light" || ($themeMode === "system" && !systemDark),
  );
  const darkActive = $derived(
    $themeMode === "dark" || ($themeMode === "system" && systemDark),
  );

  function segKeydown(
    e: KeyboardEvent,
    opts: readonly { value: string }[],
    current: string,
    select: (value: string) => void,
  ) {
    const dir =
      e.key === "ArrowRight" || e.key === "ArrowDown"
        ? 1
        : e.key === "ArrowLeft" || e.key === "ArrowUp"
          ? -1
          : 0;
    if (dir === 0) return;
    e.preventDefault();
    const values = opts.map((o) => o.value);
    const next = values[(values.indexOf(current) + dir + values.length) % values.length];
    select(next);
    const group = e.currentTarget as HTMLElement;
    group.querySelector<HTMLElement>(`[data-value="${next}"]`)?.focus();
  }
</script>

{#snippet segmented(options: SegOption[], current: string, select: (value: string) => void, testid: string)}
  <div
    role="radiogroup"
    tabindex={-1}
    data-testid="{testid}-toggle"
    class="inline-flex gap-0.5 rounded-lg border border-border bg-muted/60 p-0.5"
    onkeydown={(e) => segKeydown(e, options, current, select)}
  >
    {#each options as opt (opt.value)}
      {@const active = current === opt.value}
      {@const Icon = opt.icon}
      <button
        type="button"
        role="radio"
        aria-checked={active}
        tabindex={active ? 0 : -1}
        data-value={opt.value}
        data-testid="{testid}-{opt.value}"
        class="inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 text-label-sm ring-offset-background transition-all duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-1
          {active
            ? 'bg-card text-foreground shadow-sm'
            : 'text-muted-foreground hover:text-foreground'}"
        onclick={() => select(opt.value)}
      >
        {#if Icon}<Icon size={13} strokeWidth={1.9} />{/if}
        <span>{$t(opt.labelKey)}</span>
      </button>
    {/each}
  </div>
{/snippet}

{#snippet themeSwatch(scope: string, isDark: boolean, active: boolean, systemMatch: boolean, testid: string)}
  <div
    class="{scope} flex-1 overflow-hidden rounded-lg border bg-background transition-shadow duration-200
      {active ? 'border-primary ring-2 ring-primary/40' : 'border-border'}"
    data-testid={testid}
  >
    <div class="flex flex-col gap-2 p-3" aria-hidden="true">
      <div class="flex items-center gap-2">
        <span class="h-4 w-4 shrink-0 rounded bg-primary-gradient"></span>
        <span class="h-1.5 w-2/3 rounded-full bg-muted"></span>
      </div>
      <span class="inline-flex w-fit rounded-full bg-primary/10 px-2 py-0.5 text-overline uppercase text-primary">
        {$t($uiMode === "operator" ? "settings.mode_operator" : "settings.mode_builder")}
      </span>
      <span class="h-1.5 w-2/5 rounded-full bg-muted"></span>
      <span class="w-fit self-end rounded-lg rounded-br-sm bg-muted px-2.5 py-1 text-caption text-foreground">
        {$t("settings.appearance.preview_message")}
      </span>
    </div>
    <div class="flex items-center gap-2 border-t border-border/60 bg-card px-3 py-2 text-caption">
      {#if isDark}
        <Moon size={13} strokeWidth={1.9} class="text-muted-foreground" />
      {:else}
        <Sun size={13} strokeWidth={1.9} class="text-muted-foreground" />
      {/if}
      <span class="font-medium text-foreground">
        {$t(isDark ? "settings.theme_dark" : "settings.theme_light")}
      </span>
      {#if systemMatch}
        <span class="ml-auto rounded-full bg-muted px-2 py-0.5 text-overline uppercase text-muted-foreground">
          {$t("settings.appearance.preview_active_now")}
        </span>
      {/if}
    </div>
  </div>
{/snippet}

<SettingsSubPage route="appearance" data-testid="preferences-section">
  <!-- Language -->
  <SettingsSection title={$t("settings.language_title")} description={$t("settings.language_help")}>
    {#snippet icon()}<Globe size={15} strokeWidth={1.75} />{/snippet}
    <SettingsFieldRow
      label={$t("settings.language_label")}
      hint={$t("settings.language_field_help")}
      border={false}
    >
      {#snippet control()}
        {@render segmented(LANGUAGE_OPTIONS, currentLang, selectLocale, "language")}
      {/snippet}
    </SettingsFieldRow>
  </SettingsSection>

  <!-- Theme + focal live preview -->
  <SettingsSection title={$t("settings.theme_title")} description={$t("settings.theme_help")}>
    {#snippet icon()}<Palette size={15} strokeWidth={1.75} />{/snippet}
    {#snippet actions()}
      {@render segmented(THEME_OPTIONS, $themeMode, selectTheme, "theme")}
    {/snippet}

    <div class="flex flex-col gap-3 sm:flex-row" data-testid="theme-preview">
      {@render themeSwatch("light", false, lightActive, $themeMode === "system" && !systemDark, "theme-preview-light")}
      {@render themeSwatch("dark", true, darkActive, $themeMode === "system" && systemDark, "theme-preview-dark")}
    </div>
  </SettingsSection>

  <!-- Interface mode -->
  <SettingsSection title={$t("settings.mode_title")} description={$t("settings.mode_help")}>
    {#snippet icon()}<Eye size={15} strokeWidth={1.75} />{/snippet}
    <div
      role="radiogroup"
      tabindex={-1}
      data-testid="mode-toggle"
      class="flex flex-col gap-3 sm:flex-row"
      onkeydown={(e) => segKeydown(e, MODE_OPTIONS, $uiMode, selectMode)}
    >
      {#each MODE_OPTIONS as opt (opt.value)}
        {@const active = $uiMode === opt.value}
        {@const Icon = opt.icon}
        <button
          type="button"
          role="radio"
          aria-checked={active}
          tabindex={active ? 0 : -1}
          data-value={opt.value}
          data-testid="mode-{opt.value}"
          class="relative flex flex-1 items-start overflow-hidden rounded-lg border bg-background text-left ring-offset-background transition-all duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-2
            {active ? 'border-primary/55 ring-1 ring-primary/20' : 'border-border hover:border-primary/40'}"
          onclick={() => selectMode(opt.value)}
        >
          <span class="absolute inset-y-0 left-0 w-0.5 {active ? 'bg-primary' : 'bg-muted'}" aria-hidden="true"></span>
          <span class="flex min-w-0 flex-col gap-0.5 py-3 pl-4 pr-3">
            <span class="flex items-center gap-2">
              <Icon size={14} strokeWidth={1.9} class={active ? "text-primary" : "text-muted-foreground"} />
              <span class="text-label-md text-foreground">{$t(opt.labelKey)}</span>
              {#if active}<Check size={14} strokeWidth={2.2} class="ml-auto text-primary" />{/if}
            </span>
            <span class="text-caption text-muted-foreground">{$t(opt.descKey)}</span>
          </span>
        </button>
      {/each}
    </div>
  </SettingsSection>

  <!-- Options -->
  <SettingsSection title={$t("settings.appearance.options_title")} bodyLayout="flush">
    {#snippet icon()}<SlidersHorizontal size={15} strokeWidth={1.75} />{/snippet}
    <SettingsFieldRow
      label={$t("settings.planMode.title")}
      hint={$t("settings.planMode.description")}
      for="plan-mode-default"
    >
      {#snippet control()}
        <Toggle
          id="plan-mode-default"
          checked={$planModeDefault}
          onchange={(next) => planModeDefault.set(next)}
          aria-label={$t("settings.planMode.title")}
          data-testid="plan-mode-default-toggle"
        />
      {/snippet}
    </SettingsFieldRow>
  </SettingsSection>
</SettingsSubPage>
