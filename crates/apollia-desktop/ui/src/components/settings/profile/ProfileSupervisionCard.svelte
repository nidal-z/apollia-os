<script lang="ts">
  /**
   * ProfileSupervisionCard - the sensitive agent-supervision section: HITL
   * policy, agent domains, and trigger mode. Editing these does not re-derive
   * permission rules (see the sensitive notice); rerun onboarding to apply.
   */
  import { t } from "svelte-i18n";
  import { ShieldAlert } from "lucide-svelte";
  import { Input } from "$lib/components/ui/input";
  import { RadioGroup, RadioItem } from "$lib/components/ui/radio";
  import SettingsSection from "../SettingsSection.svelte";
  import ProfileFieldLabel from "./ProfileFieldLabel.svelte";
  import { HITL_OPTIONS, TRIGGER_OPTIONS, type ProfileFieldApi } from "./profileForm";

  interface Props {
    api: ProfileFieldApi;
  }

  let { api }: Props = $props();
</script>

<SettingsSection
  title={$t("settings.profile.supervision.title")}
  description={$t("settings.profile.sensitive_notice")}
  data-testid="profile-section-supervision"
>
  {#snippet icon()}
    <ShieldAlert size={15} strokeWidth={1.75} class="text-warning-a11y" />
  {/snippet}

  <div class="space-y-2">
    <ProfileFieldLabel
      label={$t("settings.profile.fields.hitl")}
      source={api.sources["agents.hitl"]}
      sensitive
    />
    <RadioGroup
      value={api.values["agents.hitl"] ?? ""}
      onchange={(v: string) => api.set("agents.hitl", v)}
      data-testid="profile-radio-hitl"
    >
      {#each HITL_OPTIONS as opt (opt.v)}
        <RadioItem
          value={opt.v}
          checked={api.values["agents.hitl"] === opt.v}
          onchange={(v: string) => api.set("agents.hitl", v)}
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

  <div class="space-y-1">
    <ProfileFieldLabel
      label={$t("settings.profile.fields.agent_domains")}
      source={api.sources["agents.domains"]}
      for="profile-field-agent-domains"
    />
    <Input
      id="profile-field-agent-domains"
      value={api.values["agents.domains"] ?? ""}
      oninput={(e: Event) => api.set("agents.domains", (e.target as HTMLInputElement).value)}
      placeholder={$t("settings.profile.placeholder.agent_domains")}
      data-testid="profile-input-agent-domains"
    />
    <p class="text-caption text-muted-foreground">
      {$t("settings.profile.hint.agent_domains")}
    </p>
  </div>

  <div class="space-y-2">
    <ProfileFieldLabel
      label={$t("settings.profile.fields.trigger")}
      source={api.sources["agents.trigger"]}
    />
    <RadioGroup
      value={api.values["agents.trigger"] ?? ""}
      onchange={(v: string) => api.set("agents.trigger", v)}
      class="flex-row flex-wrap gap-3"
      data-testid="profile-radio-trigger"
    >
      {#each TRIGGER_OPTIONS as opt (opt.v)}
        <RadioItem
          value={opt.v}
          checked={api.values["agents.trigger"] === opt.v}
          onchange={(v: string) => api.set("agents.trigger", v)}
          class="rounded-lg border border-border bg-surface-1 px-3 py-1.5 hover:border-primary/40"
        >
          <span class="text-body-sm text-foreground">{$t(opt.labelKey)}</span>
        </RadioItem>
      {/each}
    </RadioGroup>
  </div>
</SettingsSection>
