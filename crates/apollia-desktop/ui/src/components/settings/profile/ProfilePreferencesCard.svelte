<script lang="ts">
  /**
   * ProfilePreferencesCard - default language and model backend for agents.
   */
  import { t } from "svelte-i18n";
  import { Settings2 } from "lucide-svelte";
  import { Select } from "$lib/components/ui/select";
  import SettingsSection from "../SettingsSection.svelte";
  import ProfileFieldLabel from "./ProfileFieldLabel.svelte";
  import { LANGUAGES, LLM_BACKENDS, type ProfileFieldApi } from "./profileForm";

  interface Props {
    api: ProfileFieldApi;
  }

  let { api }: Props = $props();
</script>

<SettingsSection
  title={$t("settings.profile.preferences.title")}
  description={$t("settings.profile.preferences.subtitle")}
  data-testid="profile-section-preferences"
>
  {#snippet icon()}
    <Settings2 size={15} strokeWidth={1.75} />
  {/snippet}

  <div class="grid gap-4 md:grid-cols-2">
    <div class="space-y-1">
      <ProfileFieldLabel
        label={$t("settings.profile.fields.language")}
        source={api.sources["preferences.language"]}
        for="profile-field-language"
      />
      <Select
        id="profile-field-language"
        value={api.values["preferences.language"] ?? ""}
        onchange={(e: Event) => api.set("preferences.language", (e.target as HTMLSelectElement).value)}
        data-testid="profile-select-language"
      >
        {#each LANGUAGES as opt (opt.v)}
          <option value={opt.v}>{$t(opt.labelKey)}</option>
        {/each}
      </Select>
    </div>

    <div class="space-y-1">
      <ProfileFieldLabel
        label={$t("settings.profile.fields.llm")}
        source={api.sources["preferences.llm"]}
        for="profile-field-llm"
      />
      <Select
        id="profile-field-llm"
        value={api.values["preferences.llm"] ?? ""}
        onchange={(e: Event) => api.set("preferences.llm", (e.target as HTMLSelectElement).value)}
        data-testid="profile-select-llm"
      >
        {#each LLM_BACKENDS as opt (opt.v)}
          <option value={opt.v}>{$t(opt.labelKey)}</option>
        {/each}
      </Select>
    </div>
  </div>
</SettingsSection>
