<script module lang="ts">
  export const meta = {
    title: "settings.nav.developer",
    icon: "code",
    group: "settings.nav.cluster_system",
    cluster: "system",
  } as const;

  /** The two choices of the switch, in reading order. */
  export const DEVELOPER_MODE_CHOICES = [
    { on: false, labelKey: "settings.developer.mode_off", testid: "developer-mode-off" },
    { on: true, labelKey: "settings.developer.mode_on", testid: "developer-mode-on" },
  ] as const;
</script>

<script lang="ts">
  import { t } from "svelte-i18n";
  import { Code, Lock } from "lucide-svelte";
  import {
    developerMode,
    developerModeSource,
    setDeveloperMode,
  } from "$lib/stores/developerMode";
  import { addToast } from "$lib/components/ui/toast";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import SettingsSection from "../../components/settings/SettingsSection.svelte";
  import SettingsFieldRow from "../../components/settings/SettingsFieldRow.svelte";
  import McpServerSection from "../../components/settings/developer/McpServerSection.svelte";
  import { mcpSectionVisible } from "../../components/settings/developer/mcpServer";

  const locked = $derived($developerModeSource === "policy");

  async function choose(on: boolean) {
    if (locked || on === $developerMode) return;
    const written = await setDeveloperMode(on);
    if (!written) addToast($t("settings.developer.write_failed"), "error");
  }

  function onKeydown(e: KeyboardEvent) {
    if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(e.key)) return;
    e.preventDefault();
    const next = !$developerMode;
    void choose(next);
    const group = e.currentTarget as HTMLElement;
    group.querySelector<HTMLElement>(`[data-on="${next}"]`)?.focus();
  }
</script>

<SettingsSubPage route="developer" data-testid="developer-subpage">
  <SettingsSection title={$t("settings.developer.mode_title")}>
    {#snippet icon()}<Code size={15} strokeWidth={1.75} />{/snippet}
    <SettingsFieldRow
      label={$t("settings.developer.mode_label")}
      hint={$t("settings.developer.mode_hint")}
      border={false}
      data-testid="developer-mode-row"
    >
      {#snippet control()}
        <div class="flex flex-col items-end gap-1">
          <div
            role="radiogroup"
            tabindex={-1}
            aria-label={$t("settings.developer.mode_label")}
            aria-disabled={locked}
            data-testid="developer-mode-toggle"
            data-state={$developerMode ? "on" : "off"}
            class="inline-flex gap-0.5 rounded-lg border border-border bg-muted/60 p-0.5"
            onkeydown={onKeydown}
          >
            {#each DEVELOPER_MODE_CHOICES as choice (choice.testid)}
              {@const active = $developerMode === choice.on}
              <button
                type="button"
                role="radio"
                aria-checked={active}
                tabindex={active ? 0 : -1}
                disabled={locked}
                data-on={choice.on}
                data-testid={choice.testid}
                class="inline-flex items-center rounded-md px-3 py-1.5 text-label-sm ring-offset-background transition-all duration-base focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-1 disabled:cursor-not-allowed disabled:opacity-60
                  {active
                    ? 'bg-primary text-primary-foreground shadow-sm'
                    : 'text-muted-foreground hover:text-foreground'}"
                onclick={() => void choose(choice.on)}
              >
                {$t(choice.labelKey)}
              </button>
            {/each}
          </div>
          {#if locked}
            <p
              class="inline-flex items-center gap-1.5 text-xs text-muted-foreground"
              title={$t("settings.policy_locked_hint")}
              data-testid="developer-mode-policy"
            >
              <Lock class="h-3 w-3" aria-hidden="true" />
              {$t("settings.policy_locked")}
            </p>
          {/if}
        </div>
      {/snippet}
    </SettingsFieldRow>
  </SettingsSection>

  {#if mcpSectionVisible($developerMode)}
    <McpServerSection />
  {/if}
</SettingsSubPage>
