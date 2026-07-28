<script lang="ts">
  /**
   * ProfileConstraintsCard - the sensitive constraints section: data
   * sovereignty policy and applicable compliance frameworks.
   */
  import { t } from "svelte-i18n";
  import { Briefcase } from "lucide-svelte";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { RadioGroup, RadioItem } from "$lib/components/ui/radio";
  import SettingsSection from "../SettingsSection.svelte";
  import ProfileFieldLabel from "./ProfileFieldLabel.svelte";
  import {
    SOVEREIGNTY_OPTIONS,
    COMPLIANCE,
    listIncludes,
    toggleList,
    type ProfileFieldApi,
  } from "./profileForm";

  interface Props {
    api: ProfileFieldApi;
  }

  let { api }: Props = $props();

  function toggleNorm(entry: string, checked: boolean): void {
    api.set(
      "constraints.compliance",
      toggleList(api.values["constraints.compliance"] ?? "", entry, checked),
    );
  }
</script>

<SettingsSection
  title={$t("settings.profile.constraints.title")}
  description={$t("settings.profile.sensitive_notice")}
  data-testid="profile-section-constraints"
>
  {#snippet icon()}
    <Briefcase size={15} strokeWidth={1.75} class="text-warning-a11y" />
  {/snippet}

  <div class="space-y-2">
    <ProfileFieldLabel
      label={$t("settings.profile.fields.sovereignty")}
      source={api.sources["constraints.sovereignty"]}
      sensitive
    />
    <RadioGroup
      value={api.values["constraints.sovereignty"] ?? ""}
      onchange={(v: string) => api.set("constraints.sovereignty", v)}
      data-testid="profile-radio-sovereignty"
    >
      {#each SOVEREIGNTY_OPTIONS as opt (opt.v)}
        <RadioItem
          value={opt.v}
          checked={api.values["constraints.sovereignty"] === opt.v}
          onchange={(v: string) => api.set("constraints.sovereignty", v)}
          class="items-start rounded-lg border border-border bg-surface-1 p-3 hover:border-primary/40"
        >
          <span class="flex flex-col">
            <span class="text-body-sm font-medium text-foreground">{$t(opt.labelKey)}</span>
            {#if opt.hintKey}
              <span class="text-caption text-muted-foreground">{$t(opt.hintKey)}</span>
            {/if}
          </span>
        </RadioItem>
      {/each}
    </RadioGroup>
  </div>

  <div class="space-y-2">
    <ProfileFieldLabel
      label={$t("settings.profile.fields.compliance")}
      source={api.sources["constraints.compliance"]}
      sensitive
    />
    <div class="flex flex-wrap gap-3">
      {#each COMPLIANCE as norm (norm)}
        <label class="flex cursor-pointer items-center gap-2 text-body-sm text-foreground">
          <Checkbox
            checked={listIncludes(api.values["constraints.compliance"] ?? "", norm)}
            onchange={(c: boolean) => toggleNorm(norm, c)}
            data-testid={`profile-compliance-${norm.toLowerCase()}`}
          />
          <span>{norm}</span>
        </label>
      {/each}
    </div>
  </div>
</SettingsSection>
