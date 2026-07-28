<script lang="ts">
  /**
   * ProfileToolsCard - the tools & business-context section: daily tools, tech
   * stack, proficiency, and the (sensitive) authorized integrations.
   */
  import { t } from "svelte-i18n";
  import { Cpu } from "lucide-svelte";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import SettingsSection from "../SettingsSection.svelte";
  import ProfileFieldLabel from "./ProfileFieldLabel.svelte";
  import {
    PROFICIENCY,
    INTEGRATIONS,
    listIncludes,
    toggleList,
    type ProfileFieldApi,
  } from "./profileForm";

  interface Props {
    api: ProfileFieldApi;
  }

  let { api }: Props = $props();

  function toggleInteg(entry: string, checked: boolean): void {
    api.set("tech.integrations", toggleList(api.values["tech.integrations"] ?? "", entry, checked));
  }
</script>

<SettingsSection
  title={$t("settings.profile.tools.title")}
  description={$t("settings.profile.tools.subtitle")}
  data-testid="profile-section-tools"
>
  {#snippet icon()}
    <Cpu size={15} strokeWidth={1.75} />
  {/snippet}

  <div class="space-y-1">
    <ProfileFieldLabel
      label={$t("settings.profile.fields.tools_daily")}
      source={api.sources["tools.daily"]}
      for="profile-field-tools-daily"
    />
    <Input
      id="profile-field-tools-daily"
      value={api.values["tools.daily"] ?? ""}
      oninput={(e: Event) => api.set("tools.daily", (e.target as HTMLInputElement).value)}
      placeholder={$t("settings.profile.placeholder.tools_daily")}
      data-testid="profile-input-tools-daily"
    />
    <p class="text-caption text-muted-foreground">{$t("settings.profile.hint.tools_daily")}</p>
  </div>

  <div class="grid gap-4 md:grid-cols-2">
    <div class="space-y-1">
      <ProfileFieldLabel
        label={$t("settings.profile.fields.tech_stack")}
        source={api.sources["tech.stack"]}
        for="profile-field-tech-stack"
      />
      <Input
        id="profile-field-tech-stack"
        value={api.values["tech.stack"] ?? ""}
        oninput={(e: Event) => api.set("tech.stack", (e.target as HTMLInputElement).value)}
        placeholder={$t("settings.profile.placeholder.tech_stack")}
        data-testid="profile-input-tech-stack"
      />
    </div>

    <div class="space-y-1">
      <ProfileFieldLabel
        label={$t("settings.profile.fields.proficiency")}
        source={api.sources["tech.proficiency"]}
        for="profile-field-proficiency"
      />
      <Select
        id="profile-field-proficiency"
        value={api.values["tech.proficiency"] ?? ""}
        onchange={(e: Event) => api.set("tech.proficiency", (e.target as HTMLSelectElement).value)}
        data-testid="profile-select-proficiency"
      >
        {#each PROFICIENCY as opt (opt.v)}
          <option value={opt.v}>{$t(opt.labelKey)}</option>
        {/each}
      </Select>
    </div>
  </div>

  <div class="space-y-2">
    <ProfileFieldLabel
      label={$t("settings.profile.fields.integrations")}
      source={api.sources["tech.integrations"]}
      sensitive
    />
    <p class="text-caption text-muted-foreground">{$t("settings.profile.hint.integrations")}</p>
    <div class="flex flex-wrap gap-3">
      {#each INTEGRATIONS as integ (integ)}
        <label class="flex cursor-pointer items-center gap-2 text-body-sm text-foreground">
          <Checkbox
            checked={listIncludes(api.values["tech.integrations"] ?? "", integ)}
            onchange={(c: boolean) => toggleInteg(integ, c)}
            data-testid={`profile-integ-${integ.toLowerCase()}`}
          />
          <span>{integ}</span>
        </label>
      {/each}
    </div>
  </div>
</SettingsSection>
