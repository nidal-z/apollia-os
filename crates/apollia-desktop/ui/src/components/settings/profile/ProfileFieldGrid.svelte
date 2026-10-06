<script lang="ts">
  /**
   * ProfileFieldGrid - the form of a profile: one control per field Rust sends
   * with the profile, a single-line field for the short ones, a text area for
   * the long ones and the free text. No field is required. Settings > Profile
   * and the review of the onboarding proposal both draw this grid, so a
   * proposal is read over in the very form the profile is edited in.
   */
  import { locale, t } from "svelte-i18n";
  import { Input } from "$lib/components/ui/input";
  import { Textarea } from "$lib/components/ui/textarea";
  import type { ProfileFieldView } from "$lib/ipc/profile";
  import { fieldHelp, fieldLabel } from "./profileForm";

  interface Props {
    fields: ProfileFieldView[];
    values: Record<string, string>;
    onchange: (key: string, value: string) => void;
    disabled?: boolean;
  }

  let { fields, values, onchange, disabled = false }: Props = $props();
</script>

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
          {disabled}
          oninput={(e: Event) => onchange(field.key, (e.target as HTMLTextAreaElement).value)}
          data-testid={`profile-field-${field.key}`}
        />
      {:else}
        <Input
          value={values[field.key] ?? ""}
          placeholder={fieldHelp(field, $locale)}
          {disabled}
          oninput={(e: Event) => onchange(field.key, (e.target as HTMLInputElement).value)}
          data-testid={`profile-field-${field.key}`}
        />
      {/if}
    </label>
  {/each}
</div>
