<script lang="ts">
  /**
   * PolicyLockedNote - the mention a block of settings carries when the
   * organization policy fixes its value.
   */
  import { t } from "svelte-i18n";
  import { Lock } from "lucide-svelte";

  interface Props {
    /** Catalogue key of the mention. */
    labelKey: string;
    "data-testid"?: string;
  }

  let { labelKey, "data-testid": testid }: Props = $props();
</script>

<p
  class="inline-flex items-center gap-1.5 text-caption text-muted-foreground"
  title={$t("settings.policy_locked_hint")}
  data-testid={testid}
>
  <Lock class="h-3 w-3" aria-hidden="true" />
  {$t(labelKey)}
</p>
