<script lang="ts">
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import BuiltinToolsCatalog from "../../components/tools/BuiltinToolsCatalog.svelte";
</script>

<SettingsSubPage route="tools" data-testid="settings-tools">
  <BuiltinToolsCatalog showContract />
</SettingsSubPage>
