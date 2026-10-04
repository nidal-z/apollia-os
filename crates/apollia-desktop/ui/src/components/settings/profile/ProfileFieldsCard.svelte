<script lang="ts">
  /**
   * ProfileFieldsCard - the fields of the displayed profile, all optional,
   * drawn from the list Rust sends with the profile: a single-line field for
   * the short ones, a text area for the long ones and the free text. Edits go
   * to `values`, which the page saves with the settings frame.
   */
  import { locale, t } from "svelte-i18n";
  import { Input } from "$lib/components/ui/input";
  import { Textarea } from "$lib/components/ui/textarea";
  import type { ProfileFieldView } from "$lib/ipc/profile";
  import SettingsSection from "../SettingsSection.svelte";
  import { fieldHelp, fieldLabel } from "./profileForm";

  interface Props {
    profileName: string;
    fields: ProfileFieldView[];
    values: Record<string, string>;
    onchange: (key: string, value: string) => void;
  }

  let { profileName, fields, values, onchange }: Props = $props();
</script>

<SettingsSection
  title={$t("settings.profile.form.title", { values: { name: profileName } })}
  description={$t("settings.profile.form.description")}
  data-testid="profile-fields"
>
  <div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
    {#each fields as field (field.key)}
      <label
        class={field.long_text ? "flex flex-col gap-1.5 sm:col-span-2" : "flex flex-col gap-1.5"}
        data-testid="profile-field"
      >
        <span class="text-body-xs font-medium text-foreground">
          {fieldLabel(field, $locale)}
          <span class="font-normal text-muted-foreground">{$t("settings.profile.form.optional")}</span>
        </span>
        {#if field.long_text}
          <Textarea
            value={values[field.key] ?? ""}
            placeholder={fieldHelp(field, $locale)}
            oninput={(e: Event) => onchange(field.key, (e.target as HTMLTextAreaElement).value)}
            data-testid={`profile-field-${field.key}`}
          />
        {:else}
          <Input
            value={values[field.key] ?? ""}
            placeholder={fieldHelp(field, $locale)}
            oninput={(e: Event) => onchange(field.key, (e.target as HTMLInputElement).value)}
            data-testid={`profile-field-${field.key}`}
          />
        {/if}
      </label>
    {/each}
  </div>
  <p class="text-caption text-muted-foreground">{$t("settings.profile.form.free_text_hint")}</p>
</SettingsSection>
