<script lang="ts">
  import { t } from "svelte-i18n";
  import { MessageSquare } from "lucide-svelte";
  import { planModeDefault } from "$lib/stores/planModeSetting";
  import { Toggle } from "$lib/components/ui/toggle";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import SettingsSection from "../../components/settings/SettingsSection.svelte";
  import SettingsFieldRow from "../../components/settings/SettingsFieldRow.svelte";
  import ShortcutCatalog from "../../components/settings/shortcuts/ShortcutCatalog.svelte";
</script>

<SettingsSubPage route="chat" data-testid="chat-settings-section">
  <SettingsSection title={$t("settings.chat.conversations_title")} bodyLayout="flush">
    {#snippet icon()}<MessageSquare size={15} strokeWidth={1.75} />{/snippet}
    <SettingsFieldRow
      label={$t("settings.planMode.title")}
      hint={$t("settings.planMode.description")}
      for="plan-mode-default"
      border={false}
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

  <h2 class="px-1 pt-2 text-overline uppercase text-muted-foreground">
    {$t("settings.chat.shortcuts_title")}
  </h2>
  <ShortcutCatalog />
</SettingsSubPage>
