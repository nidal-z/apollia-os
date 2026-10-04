<script lang="ts">
  /**
   * Settings > Configuration: the editor of `apollia.toml`, its own entry of
   * the settings column, which exists only with the developer mode on. With
   * the mode off nothing is rendered, whatever asked for the section.
   */
  import { developerMode } from "$lib/stores/developerMode";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import ConfigEditor from "../../components/settings/config/ConfigEditor.svelte";
</script>

{#if $developerMode}
  <SettingsSubPage route="configuration" data-testid="configuration-subpage">
    <ConfigEditor />
  </SettingsSubPage>
{/if}
