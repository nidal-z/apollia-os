<script lang="ts">
  /**
   * OauthAdminApproval - the box the connection dialog shows when the
   * provider answers that an administrator of the user's organisation must
   * approve the application: why it is blocked, the approval address to copy
   * and send, and that a personal account is not concerned.
   */
  import { t } from "svelte-i18n";
  import { Building2, Copy } from "lucide-svelte";
  import { Banner } from "$lib/components/ui/banner";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";

  interface Props {
    /** Short provider name, `Microsoft`. */
    providerShort: string;
    /** The approval page to send the administrator. */
    address: string;
  }

  let { providerShort, address }: Props = $props();

  let copied = $state<"idle" | "done" | "failed">("idle");

  async function copy(): Promise<void> {
    try {
      await navigator.clipboard.writeText(address);
      copied = "done";
    } catch {
      copied = "failed";
    }
  }
</script>

<Banner
  variant="warning"
  surface="card"
  icon={Building2}
  title={$t("connections.oauth_admin_approval_title")}
  data-testid="oauth-admin-approval"
>
  {$t("connections.oauth_admin_approval_body", { values: { provider: providerShort } })}
</Banner>
<div class="mt-3 space-y-2">
  <p class="text-body-sm text-foreground">{$t("connections.oauth_admin_approval_address")}</p>
  <div class="flex items-center gap-2">
    <Input
      type="text"
      class="font-mono"
      readonly
      value={address}
      aria-label={$t("connections.oauth_admin_approval_address")}
      data-testid="oauth-admin-approval-address"
    />
    <Button variant="outline" size="sm" onclick={copy} data-testid="oauth-admin-approval-copy">
      {#snippet icon()}<Copy size={12} />{/snippet}
      {$t("connections.oauth_admin_approval_copy")}
    </Button>
  </div>
  {#if copied === "done"}
    <p class="text-caption text-success" role="status">{$t("connections.oauth_admin_approval_copied")}</p>
  {:else if copied === "failed"}
    <p class="text-caption text-destructive" role="status">
      {$t("connections.oauth_admin_approval_copy_failed")}
    </p>
  {/if}
  <p class="text-caption text-muted-foreground">{$t("connections.oauth_admin_approval_personal")}</p>
</div>
