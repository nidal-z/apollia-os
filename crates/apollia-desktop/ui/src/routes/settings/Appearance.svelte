<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Palette, Sun, Moon, Monitor, Lightbulb, X } from "lucide-svelte";
  import { themeMode, applyTheme, type ThemeMode } from "$lib/stores/theme";
  import {
    REMINDERS,
    readReminderStates,
    resetAllReminders,
    resetReminder,
  } from "$lib/reminders/registry";
  import { addToast } from "$lib/components/ui/toast";
  import { reportError } from "$lib/errors/reportError";
  import { Button } from "$lib/components/ui/button";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import SettingsSection from "../../components/settings/SettingsSection.svelte";
  import SettingsFieldRow from "../../components/settings/SettingsFieldRow.svelte";
  import DestructiveActionDialog from "../../components/settings/DestructiveActionDialog.svelte";
  import AppearanceChatSettings from "../../components/settings/AppearanceChatSettings.svelte";
  import { Select } from "$lib/components/ui/select";
  import {
    chatAppearance,
    setChatAppearance,
    NAV_POSITIONS,
    type NavPosition,
  } from "$lib/stores/chatAppearance";

  const NAV_POSITION_LABEL_KEYS: Record<NavPosition, string> = {
    left: "settings.appearance.nav_position_left",
    right: "settings.appearance.nav_position_right",
  };

  type Glyph = typeof Sun;
  type SegOption = { value: string; labelKey: string; icon?: Glyph };

  const THEME_OPTIONS: SegOption[] = [
    { value: "light", labelKey: "settings.theme_light", icon: Sun },
    { value: "dark", labelKey: "settings.theme_dark", icon: Moon },
    { value: "system", labelKey: "settings.theme_system", icon: Monitor },
  ];

  function selectTheme(value: string) {
    const mode = value as ThemeMode;
    themeMode.set(mode);
    applyTheme(mode);
  }

  let resetOpen = $state(false);
  let resetting = $state(false);

  let dismissed = $state<Record<string, boolean>>({});
  let resettingId = $state<string | null>(null);

  async function refreshReminders(): Promise<void> {
    dismissed = await readReminderStates();
  }

  onMount(() => {
    void refreshReminders();
  });

  async function runResetAll(): Promise<void> {
    resetting = true;
    try {
      await resetAllReminders();
      addToast($t("settings.danger.reset_onboarding.success"), "success", {
        "data-testid": "danger-reset-onboarding-toast",
      });
      resetOpen = false;
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      resetting = false;
      await refreshReminders();
    }
  }

  async function runResetOne(id: string): Promise<void> {
    resettingId = id;
    try {
      await resetReminder(id);
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      resettingId = null;
      await refreshReminders();
    }
  }

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
        class="inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 text-label-sm ring-offset-background transition-all duration-base focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-1
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
    class="{scope} flex-1 overflow-hidden rounded-lg border bg-background transition-shadow duration-base
      {active ? 'border-primary ring-2 ring-primary/40' : 'border-border'}"
    data-testid={testid}
  >
    <div class="flex flex-col gap-2 p-3" aria-hidden="true">
      <div class="flex items-center gap-2">
        <span class="h-4 w-4 shrink-0 rounded bg-primary-gradient"></span>
        <span class="h-1.5 w-2/3 rounded-full bg-muted"></span>
      </div>
      <span class="h-2.5 w-1/4 rounded-full bg-primary/10"></span>
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
        <span class="ml-auto rounded-full leading-none bg-muted px-2 py-0.5 text-overline uppercase text-muted-foreground">
          {$t("settings.appearance.preview_active_now")}
        </span>
      {/if}
    </div>
  </div>
{/snippet}

<SettingsSubPage route="appearance" data-testid="preferences-section">
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

    <SettingsFieldRow
      label={$t("settings.appearance.nav_position_label")}
      hint={$t("settings.appearance.nav_position_hint")}
      for="appearance-nav-position"
      border={false}
      data-testid="appearance-nav-position"
    >
      {#snippet control()}
        <Select
          id="appearance-nav-position"
          size="sm"
          value={$chatAppearance.navPosition}
          onchange={(e) =>
            setChatAppearance({ navPosition: e.currentTarget.value as NavPosition })}
          data-testid="appearance-nav-position-select"
        >
          {#each NAV_POSITIONS as position (position)}
            <option value={position}>{$t(NAV_POSITION_LABEL_KEYS[position])}</option>
          {/each}
        </Select>
      {/snippet}
    </SettingsFieldRow>
  </SettingsSection>

  <AppearanceChatSettings />

  <!-- Onboarding reminders -->
  <SettingsSection title={$t("settings.appearance.reminders_title")} bodyLayout="flush">
    {#snippet icon()}<Lightbulb size={15} strokeWidth={1.75} />{/snippet}
    <SettingsFieldRow
      label={$t("settings.appearance.reminders_reset_label")}
      hint={$t("settings.appearance.reminders_reset_hint")}
      border={false}
      data-testid="appearance-reset-onboarding"
    >
      {#snippet control()}
        <span id="reset-onboarding">
          <Button
            variant="outline"
            size="sm"
            onclick={() => (resetOpen = true)}
            data-testid="reset-onboarding-btn"
          >
            {$t("settings.appearance.reminders_reset_button")}
          </Button>
        </span>
      {/snippet}
    </SettingsFieldRow>
    {#each REMINDERS as reminder (reminder.id)}
      {@const label = $t(reminder.labelKey)}
      <div
        class="flex items-center justify-between gap-5 border-t border-border/60 py-2 text-sm text-muted-foreground"
        data-testid="reminder-row-{reminder.id}"
        data-dismissed={dismissed[reminder.id] ? "true" : "false"}
      >
        <span>{label}</span>
        <Button
          variant="ghost"
          size="icon-sm"
          disabled={!dismissed[reminder.id] || resettingId !== null}
          aria-label={$t("settings.appearance.reminders.reset_one", { values: { name: label } })}
          title={$t("settings.appearance.reminders.reset_one", { values: { name: label } })}
          onclick={() => runResetOne(reminder.id)}
          data-testid="reminder-reset-{reminder.id}"
        >
          <X size={14} strokeWidth={1.9} />
        </Button>
      </div>
    {/each}
  </SettingsSection>
</SettingsSubPage>

<DestructiveActionDialog
  open={resetOpen}
  title={$t("settings.danger.reset_onboarding.dialog_title")}
  description={$t("settings.danger.reset_onboarding.dialog_desc")}
  confirmWord="RESET"
  confirmLabel={$t("settings.appearance.reminders_reset_button")}
  lockMs={0}
  loading={resetting}
  onclose={() => (resetOpen = false)}
  onconfirm={runResetAll}
  data-testid="reset-onboarding-confirm"
/>
