<script lang="ts">
  /**
   * ExperimentalSection - "Experimental", in Settings > Developer.
   *
   * Three switches of the developer settings, each explained: the debugging
   * blocks of a conversation, the consumption widget of the sidebar, and the
   * load settings in presets. Then "Separate reasoning_content and content in
   * API responses", the `[server] separate_reasoning_content` key the server
   * settings show too, through the shared store.
   */
  import { t } from "svelte-i18n";
  import { CircleHelp, FlaskConical } from "lucide-svelte";
  import { addToast } from "$lib/components/ui/toast";
  import { serverSettings, setServerSetting } from "$lib/stores/serverSettings";
  import {
    developerSettings,
    setDeveloperSetting,
    type DeveloperSettingKey,
  } from "$lib/stores/developerSettings";
  import SettingsSection from "../SettingsSection.svelte";
  import SettingsToggle from "../SettingsToggle.svelte";
  import { EXPERIMENTAL_KEYS, SEPARATE_REASONING_ROW } from "./onDemandLoading";
  import {
    EXPERIMENTAL_ROWS,
    EXPERIMENTAL_WRITE_FAILED,
    type DeveloperSwitchRow,
  } from "./developerExtras";

  const row = SEPARATE_REASONING_ROW;
  const setting = $derived($serverSettings[row.key]);

  async function write(next: boolean) {
    const written = await setServerSetting(row.key, next);
    if (!written) addToast($t(EXPERIMENTAL_KEYS.writeFailed), "error");
  }

  async function writeSwitch(key: DeveloperSettingKey, next: boolean) {
    const written = await setDeveloperSetting(key, next);
    if (!written) addToast($t(EXPERIMENTAL_WRITE_FAILED), "error");
  }
</script>

{#snippet switchRow(item: DeveloperSwitchRow, testid: string)}
  {@const held = $developerSettings[item.key]}
  <div class="flex items-start gap-2 border-b border-border/60 py-1.5">
    <div class="min-w-0 flex-1">
      <SettingsToggle
        id={`${testid}-switch`}
        label={$t(item.label)}
        description={$t(item.hint)}
        checked={held.value}
        source={held.source}
        onChange={(next) => void writeSwitch(item.key, next)}
        data-testid={testid}
      />
    </div>
    {#if item.help}
      <!-- A native title: the row renders without the tooltip provider of
           the application root, as the other rows of this page do. -->
      <span
        role="img"
        class="mt-2 inline-flex cursor-help text-muted-foreground hover:text-foreground"
        title={$t(item.help.text)}
        aria-label={$t(item.help.label)}
        data-testid={`${testid}-help`}
      >
        <CircleHelp class="h-3.5 w-3.5" aria-hidden="true" />
      </span>
    {/if}
  </div>
{/snippet}

<SettingsSection
  title={$t(EXPERIMENTAL_KEYS.title)}
  bodyLayout="flush"
  data-testid="developer-experimental"
>
  {#snippet icon()}<FlaskConical size={15} strokeWidth={1.75} />{/snippet}
  {@render switchRow(EXPERIMENTAL_ROWS[0], "developer-debug-blocks")}
  {@render switchRow(EXPERIMENTAL_ROWS[1], "developer-resource-widget")}
  {@render switchRow(EXPERIMENTAL_ROWS[2], "developer-preset-load-settings")}
  <div class="py-1.5">
    <SettingsToggle
      id={`${row.testid}-switch`}
      label={$t(row.label)}
      description={$t(row.hint)}
      checked={setting.value === true}
      source={setting.source}
      onChange={(next) => void write(next)}
      data-testid={row.testid}
    />
  </div>
</SettingsSection>
