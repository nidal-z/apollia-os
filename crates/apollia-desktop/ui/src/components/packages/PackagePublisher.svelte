<script lang="ts">
  /**
   * The publisher of a package in its preview: name, seal, what the signature
   * says, the fingerprint of the key, and the gesture that trusts a signed
   * publisher not trusted yet.
   */
  import { t } from "svelte-i18n";
  import { ShieldCheck, ShieldQuestion, ShieldAlert, ShieldX } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { shortFingerprint } from "../agents/sheet/agentExport";
  import type { PackagePreview } from "$lib/ipc/packageFiles";
  import { canTrustPublisher, publisherState } from "./packagePreviewState";

  interface Props {
    preview: PackagePreview;
    trusting?: boolean;
    /** Offer the trust gesture; off where trusting would change nothing. */
    allowTrust?: boolean;
    ontrust?: () => void;
  }

  let { preview, trusting = false, allowTrust = true, ontrust }: Props = $props();

  const seal = $derived(publisherState(preview));
  const Seal = $derived(
    seal === "trusted"
      ? ShieldCheck
      : seal === "signed_untrusted"
        ? ShieldQuestion
        : seal === "unsigned"
          ? ShieldAlert
          : ShieldX,
  );
  const tone = $derived(
    seal === "trusted"
      ? "text-success-a11y"
      : seal === "invalid"
        ? "text-danger-a11y"
        : seal === "unsigned"
          ? "text-warning-a11y"
          : "text-muted-foreground",
  );
</script>

<section class="flex flex-col gap-2" data-testid="package-preview-publisher" data-state={seal}>
  <h3 class="text-caption font-medium uppercase tracking-wide text-muted-foreground">
    {$t("packages.preview.publisher.title")}
  </h3>
  <div class="flex items-start gap-2.5">
    <Seal size={18} class="mt-0.5 shrink-0 {tone}" />
    <div class="min-w-0 flex-1">
      <p class="text-body-sm font-medium text-foreground" data-testid="package-preview-publisher-name">
        {seal === "unsigned" ? $t("packages.preview.publisher.unknown") : preview.signature.publisher}
      </p>
      <p class="text-caption {tone}" data-testid="package-preview-publisher-state">
        {$t(`packages.preview.publisher.${seal}`)}
      </p>
      {#if seal === "unsigned" && preview.signature.declaredPublisher}
        <p class="text-caption text-muted-foreground">
          {$t("packages.preview.publisher.declared", {
            values: { name: preview.signature.declaredPublisher },
          })}
        </p>
      {/if}
      {#if preview.signature.keyFingerprint}
        <p class="mt-1 font-mono text-code-sm text-muted-foreground" data-testid="package-preview-fingerprint">
          {$t("packages.preview.publisher.fingerprint")}
          {shortFingerprint(preview.signature.keyFingerprint)}
        </p>
      {/if}
    </div>
  </div>
  {#if allowTrust && ontrust && canTrustPublisher(preview)}
    <div class="flex flex-col items-start gap-1.5">
      <Button
        variant="outline"
        size="sm"
        onclick={ontrust}
        disabled={trusting}
        data-testid="package-preview-trust-button"
      >
        {$t("packages.preview.publisher.trust_button")}
      </Button>
      <p class="text-caption text-muted-foreground">{$t("packages.preview.publisher.trust_note")}</p>
    </div>
  {/if}
</section>
