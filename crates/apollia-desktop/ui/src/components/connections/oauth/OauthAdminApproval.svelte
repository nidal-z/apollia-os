<script lang="ts">
  /**
   * OauthAdminApproval - the box the connection dialog shows when the
   * provider answers that an administrator of the user's organisation must
   * approve the application (`required`): why it is blocked, the approval
   * address to copy and send, and that a personal account is not concerned.
   * When a work account refused the consent (`possible`), which may hide an
   * approval left unasked, the same address is offered, conditionally, and
   * nothing is said to be blocked. A request is written for the
   * administrator, in the interface language, the address in it, to copy and
   * send as it is.
   */
  import { t } from "svelte-i18n";
  import { Building2, Copy } from "lucide-svelte";
  import { Banner } from "$lib/components/ui/banner";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Textarea } from "$lib/components/ui/textarea";

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
  let requestCopied = $state<"idle" | "done" | "failed">("idle");

  const request = $derived(
    $t("connections.oauth_admin_approval_request", { values: { provider: providerShort, address } }),
  );

  async function copyRequest(): Promise<void> {
    try {
      await navigator.clipboard.writeText(request);
      requestCopied = "done";
    } catch {
      requestCopied = "failed";
    }
  }

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
  <p class="pt-1 text-body-sm text-foreground">{$t("connections.oauth_admin_approval_request_label")}</p>
  <Textarea
    readonly
    rows={6}
    value={request}
    aria-label={$t("connections.oauth_admin_approval_request_label")}
    data-testid="oauth-admin-approval-request"
  />
  <div class="flex items-center gap-2">
    <Button variant="outline" size="sm" onclick={copyRequest} data-testid="oauth-admin-approval-request-copy">
      {#snippet icon()}<Copy size={12} />{/snippet}
      {$t("connections.oauth_admin_approval_request_copy")}
    </Button>
    {#if requestCopied === "done"}
      <p class="text-caption text-success" role="status">{$t("connections.oauth_admin_approval_request_copied")}</p>
    {:else if requestCopied === "failed"}
      <p class="text-caption text-destructive" role="status">{$t("connections.oauth_admin_approval_copy_failed")}</p>
    {/if}
  </div>
  {#if kind === "required"}
    <p class="text-caption text-muted-foreground">{$t("connections.oauth_admin_approval_personal")}</p>
  {/if}
</div>
