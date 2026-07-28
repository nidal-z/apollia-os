<script lang="ts">
  /**
   * ProfileIdentityCard - the identity section: a warm terracotta avatar focal
   * (the single warm accent on an otherwise cool surface) alongside the
   * name / role / sector / team-size / goals fields.
   */
  import { t } from "svelte-i18n";
  import { User, Camera } from "lucide-svelte";
  import { Input } from "$lib/components/ui/input";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Select } from "$lib/components/ui/select";
  import { avatarHue } from "$lib/components/ui/avatar";
  import SettingsSection from "../SettingsSection.svelte";
  import ProfileFieldLabel from "./ProfileFieldLabel.svelte";
  import {
    SECTORS,
    TEAM_SIZES,
    LANGUAGES,
    optionLabelKey,
    type ProfileFieldApi,
  } from "./profileForm";

  interface Props {
    api: ProfileFieldApi;
  }

  let { api }: Props = $props();

  const name = $derived(api.values["name"] ?? "");
  const role = $derived(api.values["role"] ?? "");

  const initials = $derived.by(() => {
    const parts = name.trim().split(/\s+/).filter(Boolean);
    if (parts.length === 0) return "";
    if (parts.length === 1) return parts[0].charAt(0).toUpperCase();
    return (parts[0].charAt(0) + parts[parts.length - 1].charAt(0)).toUpperCase();
  });

  // Kept for parity with sibling avatar decorations elsewhere in the app.
  const hue = $derived(avatarHue(name));

  const chips = $derived(
    [
      optionLabelKey(SECTORS, api.values["domain.sector"] ?? ""),
      optionLabelKey(TEAM_SIZES, api.values["domain.team_size"] ?? ""),
      optionLabelKey(LANGUAGES, api.values["preferences.language"] ?? ""),
    ].filter((k): k is string => k !== null),
  );

  function onInput(key: string, e: Event): void {
    api.set(key, (e.target as HTMLInputElement | HTMLTextAreaElement).value);
  }

  function onSelect(key: string, e: Event): void {
    api.set(key, (e.target as HTMLSelectElement).value);
  }
</script>

<SettingsSection
  title={$t("settings.profile.identity.title")}
  description={$t("settings.profile.identity.subtitle")}
  data-testid="profile-section-identity"
>
  {#snippet icon()}
    <User size={15} strokeWidth={1.75} />
  {/snippet}

  <!-- Warm avatar focal + resolved identity summary. -->
  <div class="flex items-center gap-4 pb-1">
    <div class="group relative shrink-0">
      <button
        type="button"
        style="--agent-hue: {hue};"
        class="relative grid h-16 w-16 place-items-center rounded-2xl bg-[image:var(--avatar-gradient-warm)] text-heading-md font-bold tracking-tight text-primary-foreground shadow-elev-2 transition-all duration-base ease-apple hover:-translate-y-0.5 hover:scale-[1.02] hover:shadow-warm-focus focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background"
        aria-label={$t("settings.profile.avatar_change")}
        data-testid="profile-avatar"
      >
        {#if initials}
          {initials}
        {:else}
          <User size={26} strokeWidth={1.5} />
        {/if}
        <span
          class="absolute -bottom-1 -right-1 grid h-6 w-6 place-items-center rounded-full border border-border bg-card text-muted-foreground opacity-0 transition-opacity duration-fast ease-apple group-hover:opacity-100 group-focus-within:opacity-100"
        >
          <Camera size={12} strokeWidth={1.9} aria-hidden="true" />
        </span>
      </button>
    </div>

    <div class="min-w-0">
      <div class="truncate text-heading-sm text-foreground">
        {name.trim() || $t("settings.profile.identity.unnamed")}
      </div>
      {#if role.trim()}
        <div class="mt-0.5 text-body-sm text-muted-foreground">{role}</div>
      {:else}
        <div class="mt-0.5 text-body-sm text-muted-foreground">
          {$t("settings.profile.identity.fill_prompt")}
        </div>
      {/if}
      {#if chips.length > 0}
        <div class="mt-2 flex flex-wrap gap-1.5">
          {#each chips as chipKey (chipKey)}
            <span
              class="rounded-full border border-border/70 bg-surface-2 px-2 py-0.5 text-caption text-muted-foreground"
            >
              {$t(chipKey)}
            </span>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  <div class="grid gap-4 md:grid-cols-2">
    <div class="space-y-1">
      <ProfileFieldLabel
        label={$t("settings.profile.fields.name")}
        source={api.sources["name"]}
        for="profile-field-name"
      />
      <Input
        id="profile-field-name"
        value={name}
        oninput={(e: Event) => onInput("name", e)}
        placeholder={$t("settings.profile.placeholder.name")}
        data-testid="profile-input-name"
      />
    </div>

    <div class="space-y-1">
      <ProfileFieldLabel
        label={$t("settings.profile.fields.role")}
        source={api.sources["role"]}
        for="profile-field-role"
      />
      <Input
        id="profile-field-role"
        value={role}
        oninput={(e: Event) => onInput("role", e)}
        placeholder={$t("settings.profile.placeholder.role")}
        data-testid="profile-input-role"
      />
    </div>

    <div class="space-y-1">
      <ProfileFieldLabel
        label={$t("settings.profile.fields.sector")}
        source={api.sources["domain.sector"]}
        for="profile-field-sector"
      />
      <Select
        id="profile-field-sector"
        value={api.values["domain.sector"] ?? ""}
        onchange={(e: Event) => onSelect("domain.sector", e)}
        data-testid="profile-select-sector"
      >
        {#each SECTORS as opt (opt.v)}
          <option value={opt.v}>{$t(opt.labelKey)}</option>
        {/each}
      </Select>
    </div>

    <div class="space-y-1">
      <ProfileFieldLabel
        label={$t("settings.profile.fields.team_size")}
        source={api.sources["domain.team_size"]}
        for="profile-field-team-size"
      />
      <Select
        id="profile-field-team-size"
        value={api.values["domain.team_size"] ?? ""}
        onchange={(e: Event) => onSelect("domain.team_size", e)}
        data-testid="profile-select-team-size"
      >
        {#each TEAM_SIZES as opt (opt.v)}
          <option value={opt.v}>{$t(opt.labelKey)}</option>
        {/each}
      </Select>
    </div>
  </div>

  <div class="space-y-1">
    <ProfileFieldLabel
      label={$t("settings.profile.fields.goals")}
      source={api.sources["goals"]}
      for="profile-field-goals"
    />
    <Textarea
      id="profile-field-goals"
      value={api.values["goals"] ?? ""}
      oninput={(e: Event) => onInput("goals", e)}
      placeholder={$t("settings.profile.placeholder.goals")}
      rows={2}
      data-testid="profile-textarea-goals"
    />
  </div>
</SettingsSection>
