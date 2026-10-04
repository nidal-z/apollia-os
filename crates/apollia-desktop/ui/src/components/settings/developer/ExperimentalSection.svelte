<script lang="ts">
  /**
   * ExperimentalSection - "Experimental", in Settings > Developer.
   *
   * Carries "Separate reasoning_content and content in API responses", the
   * `[server] separate_reasoning_content` key the server settings show too,
   * through the shared store.
   */
  import { t } from "svelte-i18n";
  import { FlaskConical } from "lucide-svelte";
  import { addToast } from "$lib/components/ui/toast";
  import { serverSettings, setServerSetting } from "$lib/stores/serverSettings";
  import SettingsSection from "../SettingsSection.svelte";
  import SettingsToggle from "../SettingsToggle.svelte";
  import { EXPERIMENTAL_KEYS, SEPARATE_REASONING_ROW } from "./onDemandLoading";

  const row = SEPARATE_REASONING_ROW;
  const setting = $derived($serverSettings[row.key]);

  async function write(next: boolean) {
    const written = await setServerSetting(row.key, next);
    if (!written) addToast($t(EXPERIMENTAL_KEYS.writeFailed), "error");
  }
</script>

<SettingsSection title={$t(EXPERIMENTAL_KEYS.title)} data-testid="developer-experimental">
  {#snippet icon()}<FlaskConical size={15} strokeWidth={1.75} />{/snippet}
  <SettingsToggle
    id={`${row.testid}-switch`}
    label={$t(row.label)}
    description={$t(row.hint)}
    checked={setting.value === true}
    source={setting.source}
    onChange={(next) => void write(next)}
    data-testid={row.testid}
  />
</SettingsSection>
