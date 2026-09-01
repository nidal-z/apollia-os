<script lang="ts">
  /**
   * ProfileFieldLabel - the label line above a profile control: text, an
   * optional provenance badge, and an optional "sensitive" marker. Rendered as
   * a plain label (associates with the control via `for` when an id is given).
   */
  import { t } from "svelte-i18n";
  import ProfileSourceBadge from "./ProfileSourceBadge.svelte";

  interface Props {
    /** Already-translated label text. */
    label: string;
    /** Raw `written_by` provenance for the badge. */
    source?: string | undefined;
    /** Show the sensitive marker. */
    sensitive?: boolean;
    /** Associate the label with a control id. */
    for?: string;
  }

  let { label, source = undefined, sensitive = false, for: htmlFor }: Props = $props();
</script>

<label
  for={htmlFor}
  class="flex flex-wrap items-center gap-1.5 text-label-sm font-medium text-muted-foreground"
>
  {label}
  <ProfileSourceBadge {source} />
  {#if sensitive}
    <span
      class="rounded-full leading-none bg-warning/15 px-1.5 py-px text-overline font-semibold uppercase tracking-wider text-warning-a11y"
    >
      {$t("settings.profile.sensitive_badge")}
    </span>
  {/if}
</label>
