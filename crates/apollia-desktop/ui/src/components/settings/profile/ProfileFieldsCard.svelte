<script lang="ts">
  /**
   * ProfileFieldsCard - the fields of the displayed profile, all optional,
   * drawn by {@link ProfileFieldGrid} from the list Rust sends with the
   * profile. Edits go to `values`, which the page saves with the settings
   * frame.
   */
  import { t } from "svelte-i18n";
  import type { ProfileFieldView } from "$lib/ipc/profile";
  import SettingsSection from "../SettingsSection.svelte";
  import ProfileFieldGrid from "./ProfileFieldGrid.svelte";

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
  <ProfileFieldGrid {fields} {values} {onchange} />
  <p class="text-caption text-muted-foreground">{$t("settings.profile.form.free_text_hint")}</p>
</SettingsSection>
