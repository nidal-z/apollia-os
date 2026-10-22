<script lang="ts">
  /**
   * OauthAdminApproval - the box the connection dialog shows when the
   * provider answers that an administrator of the user's organisation must
   * approve the application (`required`): why it is blocked, the approval
   * address to copy and send, and that a personal account is not concerned.
   * When a work account refused the consent (`possible`), which may hide an
   * approval left unasked, the same address is offered, conditionally, and
   * nothing is said to be blocked.
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
    /** Whether the approval is required, or only possibly what is missing. */
    kind?: "required" | "possible";
  }

  let { providerShort, address, kind = "required" }: Props = $props();

  let copied = $state<"idle" | "done" | "failed">("idle");

  const addressLabel = $derived(
    kind === "required"
      ? $t("connections.oauth_admin_approval_address")
      : $t("connections.oauth_admin_approval_possible"),
  );

  async function copy(): Promise<void> {
    try {
      await navigator.clipboard.writeText(address);
      copied = "done";
    } catch {
      copied = "failed";
    }
  }
</script>

{#if kind === "required"}
  <Banner
    variant="warning"
    surface="card"
    icon={Building2}
    title={$t("connections.oauth_admin_approval_title")}
    data-testid="oauth-admin-approval"
  >
    {$t("connections.oauth_admin_approval_body", { values: { provider: providerShort } })}
  </Banner>
{:else}
  <Banner
    variant="info"
    surface="card"
    icon={Building2}
    title={$t("connections.oauth_admin_approval_declined_title")}
    data-testid="oauth-admin-approval-possible"
  >
    {$t("connections.oauth_admin_approval_declined_body", { values: { provider: providerShort } })}
  </Banner>
{/if}
<div class="mt-3 space-y-2">
  <p class="text-body-sm text-foreground">{addressLabel}</p>
  <div class="flex items-center gap-2">
    <Input
      type="text"
      class="font-mono"
      readonly
      value={address}
      aria-label={addressLabel}
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
  {#if kind === "required"}
    <p class="text-caption text-muted-foreground">{$t("connections.oauth_admin_approval_personal")}</p>
  {/if}
</div>
