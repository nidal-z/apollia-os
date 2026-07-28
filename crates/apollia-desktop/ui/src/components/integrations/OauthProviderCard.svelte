<script lang="ts">
  /**
   * OauthProviderCard - the card shell for one connector's OAuth client
   * credentials. Renders the provider identity, a derived readiness pill, the
   * credential rows (via the `children` snippet), an optional loopback
   * redirect-URI note, and a footer note. Purely presentational: the rows own
   * their own save/delete flows.
   */
  import type { Snippet } from "svelte";
  import { t } from "svelte-i18n";
  import { CheckCircle2, AlertTriangle, XCircle, Lock, Info, Link2, Check } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { oauthTestClient, type OauthClientIdStatus } from "$lib/ipc/oauthClients";
  import { reportError } from "$lib/errors/reportError";

  interface Props {
    status: OauthClientIdStatus;
    name: string;
    meta: string;
    /** Loopback redirect URI to advertise; omit to hide the note (Microsoft). */
    redirectUri?: string;
    /** Footer note text. */
    footerNote: string;
    /** Whether the footer note is neutral info (Microsoft) vs security (Google). */
    footerInfo?: boolean;
    children: Snippet;
  }

  let {
    status,
    name,
    meta,
    redirectUri,
    footerNote,
    footerInfo = false,
    children,
  }: Props = $props();

  type Readiness = "ready" | "partial" | "none";

  const readiness = $derived<Readiness>(
    status.source === "none"
      ? "none"
      : status.requires_client_secret && !status.has_client_secret
        ? "partial"
        : "ready",
  );

  const monogram = $derived(name.charAt(0).toUpperCase());

  // Transient result of the non-interactive configuration check. `valid` = the
  // credentials look usable and the OIDC discovery endpoint answered; `warning`
  // = the check ran but the config is incomplete/invalid; `error` = the command
  // itself failed. Never claims a live connection: it validates presence, shape
  // and reachability only.
  type TestOutcome = "idle" | "testing" | "valid" | "warning" | "error";
  let testOutcome = $state<TestOutcome>("idle");
  let testMessage = $state("");

  const hasClientId = $derived(status.effective_client_id.length > 0);

  const testResultClass = $derived(
    testOutcome === "valid"
      ? "border-success/28 bg-success/12 text-success-a11y"
      : testOutcome === "warning"
        ? "border-warning/30 bg-warning/13 text-warning-a11y"
        : "border-destructive/28 bg-destructive/10 text-danger-a11y",
  );

  async function runTest(): Promise<void> {
    testOutcome = "testing";
    testMessage = "";
    try {
      const result = await oauthTestClient(status.provider);
      testOutcome = result.ok ? "valid" : "warning";
      testMessage = result.detail;
    } catch (err) {
      const humanized = reportError(err, { surface: "inline" });
      testOutcome = "error";
      testMessage = humanized.friendly_message;
    }
  }

  let copied = $state(false);
  async function copyRedirect(): Promise<void> {
    if (!redirectUri) return;
    try {
      await navigator.clipboard.writeText(redirectUri);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      copied = false;
    }
  }
</script>

<div
  class="overflow-hidden rounded-xl border border-border bg-card shadow-sm"
  data-testid={`oauth-card-${status.provider}`}
>
  <header
    class="flex items-start justify-between gap-3.5 border-b border-border/60 px-4 pb-3 pt-4"
  >
    <div class="flex min-w-0 items-center gap-3">
      <span
        class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-2 text-body-md font-bold text-foreground"
        aria-hidden="true"
      >
        {monogram}
      </span>
      <div class="min-w-0">
        <div class="truncate text-label-md font-semibold text-foreground">{name}</div>
        <div class="truncate text-caption text-muted-foreground">{meta}</div>
      </div>
    </div>

    <div class="flex shrink-0 flex-col items-end gap-2">
      {#if readiness === "ready"}
        <span
          class="inline-flex items-center gap-1.5 rounded-full border border-success/28 bg-success/12 px-2.5 py-1 text-caption font-semibold text-success-a11y"
        >
          <CheckCircle2 size={12} strokeWidth={2} aria-hidden="true" />
          {$t("settings.integrations.status.ready")}
        </span>
      {:else if readiness === "partial"}
        <span
          class="inline-flex items-center gap-1.5 rounded-full border border-warning/30 bg-warning/13 px-2.5 py-1 text-caption font-semibold text-warning-a11y"
        >
          <AlertTriangle size={12} strokeWidth={2} aria-hidden="true" />
          {$t("settings.integrations.status.partial")}
        </span>
      {:else}
        <span
          class="inline-flex items-center gap-1.5 rounded-full border border-destructive/28 bg-destructive/10 px-2.5 py-1 text-caption font-semibold text-danger-a11y"
        >
          <XCircle size={12} strokeWidth={2} aria-hidden="true" />
          {$t("settings.integrations.status.none")}
        </span>
      {/if}

      <Button
        variant="outline"
        size="sm"
        class="h-7 px-2.5 text-caption"
        onclick={runTest}
        loading={testOutcome === "testing"}
        disabled={!hasClientId || testOutcome === "testing"}
        data-testid={`oauth-test-${status.provider}`}
      >
        {$t("settings.integrations.test_config")}
      </Button>
    </div>
  </header>

  {#if testOutcome !== "idle" && testOutcome !== "testing"}
    <div
      class="flex items-start gap-2 border-b border-border/60 px-4 py-2.5 text-caption {testResultClass}"
      role="status"
      aria-live="polite"
      data-testid={`oauth-test-result-${status.provider}`}
    >
      {#if testOutcome === "valid"}
        <CheckCircle2 size={13} strokeWidth={2} class="mt-px shrink-0" aria-hidden="true" />
      {:else if testOutcome === "warning"}
        <AlertTriangle size={13} strokeWidth={2} class="mt-px shrink-0" aria-hidden="true" />
      {:else}
        <XCircle size={13} strokeWidth={2} class="mt-px shrink-0" aria-hidden="true" />
      {/if}
      <span class="min-w-0">
        <span class="font-semibold">
          {testOutcome === "valid"
            ? $t("settings.integrations.test_valid")
            : $t("settings.integrations.test_invalid")}
        </span>
        {#if testMessage}
          <span class="opacity-90">{testMessage}</span>
        {/if}
      </span>
    </div>
  {/if}

  <div class="flex flex-col px-4 pb-1 pt-1">
    {@render children()}

    {#if redirectUri}
      <div
        class="my-3 flex items-start gap-2.5 rounded-md border border-primary/20 bg-primary/5 px-3 py-2.5 text-caption text-muted-foreground"
      >
        <Link2 size={15} strokeWidth={1.75} class="mt-px shrink-0 text-primary" aria-hidden="true" />
        <div class="min-w-0">
          <span>{$t("settings.integrations.redirect_note")}</span>
          <div class="mt-1.5 flex flex-wrap items-center gap-2">
            <code class="rounded bg-muted px-1.5 py-0.5 font-mono text-caption text-foreground">
              {redirectUri}
            </code>
            <Button variant="outline" size="sm" onclick={copyRedirect} class="h-6 px-2 text-caption">
              {#if copied}
                <Check size={12} strokeWidth={2.5} class="mr-1" aria-hidden="true" />
                {$t("settings.integrations.copied")}
              {:else}
                {$t("common.copy")}
              {/if}
            </Button>
          </div>
        </div>
      </div>
    {/if}
  </div>

  <footer
    class="flex items-center gap-2 border-t border-border/60 bg-surface-2/40 px-4 py-2.5 text-caption text-muted-foreground"
  >
    {#if footerInfo}
      <Info size={13} strokeWidth={1.75} class="shrink-0" aria-hidden="true" />
    {:else}
      <Lock size={13} strokeWidth={1.75} class="shrink-0" aria-hidden="true" />
    {/if}
    <span>{footerNote}</span>
  </footer>
</div>
