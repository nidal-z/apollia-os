<script lang="ts">
  import { t } from "svelte-i18n";
  import { SlidersHorizontal } from "lucide-svelte";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import SettingsSection from "../../components/settings/SettingsSection.svelte";
</script>

<SettingsSubPage route="model-defaults" data-testid="model-defaults-section">
  <SettingsSection title={$t("settings.nav.model_defaults")}>
    {#snippet icon()}<SlidersHorizontal size={15} strokeWidth={1.75} />{/snippet}
    <p class="text-body-sm text-muted-foreground" data-testid="model-defaults-empty">
      {$t("settings.model_defaults.empty")}
    </p>
  </SettingsSection>
</SettingsSubPage>
