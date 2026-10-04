<script lang="ts">
  /**
   * PolicyScreen - the organisation policy the runtime applies, read only, in
   * its three states: none (the person is free), active (who signed, when,
   * and every rule in plain words), invalid (why it is refused and what holds
   * meanwhile). Nothing here changes the policy: IT writes and signs it on
   * the command line, outside this application.
   */
  import { t, locale } from "svelte-i18n";
  import { ShieldCheck, ShieldAlert, Copy } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast/store";
  import type { PolicyView } from "$lib/ipc/policy";
  import {
    formatSignedAt,
    invalidReasonKey,
    policySections,
    type PolicyRuleLine,
  } from "$lib/policy/policyRules";
  import PolicySection from "./PolicySection.svelte";

  let { view }: { view: PolicyView } = $props();

  const sections = $derived(view.state === "active" ? policySections(view.policy) : []);

  const signatureRules = $derived.by((): PolicyRuleLine[] => {
    if (view.state !== "active") return [];
    return [
      {
        id: "signed-at",
        labelKey: "policy.signature.signed_at",
        value: formatSignedAt(view.signed_at, $locale ?? "fr"),
      },
      { id: "publisher", labelKey: "policy.signature.publisher", value: view.publisher },
      { id: "verified", labelKey: "policy.signature.verified", valueKey: "policy.signature.verified_value" },
    ];
  });

  const signatureDetails = $derived.by((): PolicyRuleLine[] => {
    if (view.state !== "active") return [];
    return [
      { id: "fingerprint", labelKey: "policy.signature.fingerprint", value: view.key_fingerprint, code: true },
      { id: "location", labelKey: "policy.file.location", value: view.dir, code: true },
    ];
  });

  const reasonText = $derived(view.state === "invalid" ? $t(invalidReasonKey(view.reason)) : "");

  async function copyReason(): Promise<void> {
    try {
      await navigator.clipboard.writeText(reasonText);
      addToast($t("policy.invalid.copied"), "success");
    } catch {
      addToast($t("policy.invalid.copy_failed"), "error");
    }
  }
</script>

<div class="flex flex-col gap-4" data-testid="policy-screen" data-policy-state={view.state}>
  <header class="flex flex-col gap-2">
    <div class="flex flex-wrap items-center gap-2">
      <h1 class="text-display-sm font-semibold text-foreground">{$t("policy.title")}</h1>
      {#if view.state === "none"}
        <Badge variant="neutral" data-testid="policy-badge-state">{$t("policy.badge.none")}</Badge>
      {:else if view.state === "active"}
        <Badge variant="success" data-testid="policy-badge-state">{$t("policy.badge.active")}</Badge>
        <Badge variant="outline" data-testid="policy-badge-read-only">{$t("policy.badge.read_only")}</Badge>
      {:else}
        <Badge variant="danger" data-testid="policy-badge-state">{$t("policy.badge.invalid")}</Badge>
        <Badge variant="outline" data-testid="policy-badge-read-only">{$t("policy.badge.read_only")}</Badge>
      {/if}
    </div>
    {#if view.state !== "none"}
      <p class="text-sm text-muted-foreground" data-testid="policy-subtitle">{$t("policy.subtitle")}</p>
    {/if}
  </header>

  {#if view.state === "none"}
    <div
      class="flex flex-col items-center gap-2 rounded-lg border border-border bg-card px-6 py-12 text-center"
      data-testid="policy-none"
    >
      <ShieldCheck size={32} class="text-muted-foreground" aria-hidden="true" />
      <p class="text-base font-semibold text-foreground">{$t("policy.none.title")}</p>
      <p class="text-sm text-muted-foreground">{$t("policy.none.body")}</p>
    </div>
  {:else if view.state === "active"}
    <PolicySection
      id="signature"
      title={$t("policy.signature.title")}
      description={$t("policy.signature.description")}
      rules={signatureRules}
      details={signatureDetails}
    />
    {#each sections as section (section.id)}
      <PolicySection
        id={section.id}
        title={$t(`policy.section.${section.id}.title`)}
        description={$t(`policy.section.${section.id}.description`)}
        rules={section.rules}
      />
    {:else}
      <p class="text-sm text-muted-foreground" data-testid="policy-no-rule">{$t("policy.no_rule")}</p>
    {/each}
    <p class="text-sm text-muted-foreground" data-testid="policy-footer">{$t("policy.footer_active")}</p>
  {:else}
    <PolicySection id="reason" title={$t("policy.invalid.title")}>
      <div class="mt-2 flex items-start gap-2">
        <ShieldAlert size={16} class="mt-0.5 shrink-0 text-destructive" aria-hidden="true" />
        <p class="text-sm text-foreground" data-testid="policy-invalid-reason">{reasonText}</p>
      </div>
      <p class="mt-2 text-sm text-muted-foreground">{$t("policy.invalid.not_read")}</p>
    </PolicySection>
    <PolicySection
      id="meanwhile"
      title={$t("policy.invalid.meanwhile_title")}
      rules={[
        { id: "meanwhile-sensitive", labelKey: "policy.invalid.meanwhile_sensitive", valueKey: "policy.value.refused_feminine" },
        { id: "meanwhile-connections", labelKey: "policy.invalid.meanwhile_connections", valueKey: "policy.value.refused_feminine" },
        { id: "meanwhile-read", labelKey: "policy.invalid.meanwhile_read", valueKey: "policy.value.continues" },
      ]}
    />
    <PolicySection
      id="file"
      title={$t("policy.file.title")}
      rules={[{ id: "read-at", labelKey: "policy.file.read_at", valueKey: "policy.file.read_at_value" }]}
      details={[{ id: "location", labelKey: "policy.file.location", value: view.dir, code: true }]}
    />
    <div class="flex flex-wrap items-center justify-between gap-3">
      <p class="text-sm text-muted-foreground" data-testid="policy-footer">{$t("policy.footer_invalid")}</p>
      <Button size="sm" variant="outline" onclick={() => void copyReason()} data-testid="policy-copy-reason">
        <Copy size={14} aria-hidden="true" />
        {$t("policy.invalid.copy_reason")}
      </Button>
    </div>
  {/if}
</div>
