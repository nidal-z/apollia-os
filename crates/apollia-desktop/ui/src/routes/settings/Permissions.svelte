<script lang="ts">
  /**
   * Settings > Permissions - where the authorizations went. The fine rules,
   * the standing consents and their withdrawal live on the Authorizations
   * screen, agent by agent; this page says so and leads there.
   */
  import { t } from "svelte-i18n";
  import { KeyRound } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { navigateTo } from "$lib/stores/navigation";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import SettingsSection from "../../components/settings/SettingsSection.svelte";
</script>

<SettingsSubPage route="permissions" data-testid="settings-permissions">
  <SettingsSection title={$t("authorizations.title")} data-testid="settings-permissions-moved">
    {#snippet icon()}<KeyRound size={15} strokeWidth={1.75} />{/snippet}
    <p class="m-0 text-body-sm text-muted-foreground">{$t("settings.permissions.moved")}</p>
    <div class="mt-3">
      <Button size="sm" onclick={() => navigateTo("authorizations")} data-testid="settings-permissions-open">
        {$t("settings.permissions.open_authorizations")}
      </Button>
    </div>
  </SettingsSection>
</SettingsSubPage>
