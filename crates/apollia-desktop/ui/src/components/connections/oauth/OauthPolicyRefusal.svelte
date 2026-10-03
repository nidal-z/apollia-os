<script lang="ts">
  /**
   * OauthPolicyRefusal - the box the connection dialog shows when the
   * organisation policy refuses a native account: what was refused, that
   * nothing left the machine, and whom to ask for an exception.
   */
  import { t } from "svelte-i18n";
  import { AlertTriangle } from "lucide-svelte";
  import { Banner } from "$lib/components/ui/banner";

  interface Props {
    /** Short provider name, `Google` or `Microsoft`. */
    providerShort: string;
  }

  let { providerShort }: Props = $props();
</script>

<Banner
  variant="destructive"
  surface="card"
  icon={AlertTriangle}
  title={$t("connections.oauth_policy_refused_title")}
  data-testid="oauth-policy-refused"
>
  {$t("connections.oauth_policy_refused_body", { values: { provider: providerShort } })}
</Banner>
<p class="mt-3 text-body-sm text-muted-foreground" data-testid="oauth-policy-refused-contact">
  {$t("connections.oauth_policy_refused_contact")}
</p>
