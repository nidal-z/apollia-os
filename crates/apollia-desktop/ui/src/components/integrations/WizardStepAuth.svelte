<script lang="ts">
  import { t } from "svelte-i18n";
  import { Eye, EyeOff, ExternalLink, Lock, LogIn, Monitor, ShieldCheck } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Spinner } from "$lib/components/ui/progress";
  import { Textarea } from "$lib/components/ui/textarea";
  import { FormField } from "$lib/components/ui/form-field";
  import { handleExternalLinkClick } from "$lib/utils/externalLink";
  import type {
    ConnectorEnrichmentView,
    McpOAuthAccount,
    McpOAuthDiscoveryResult,
    RegistryEnvVarView,
    RegistryPackageArgView,
  } from "$lib/types";

  interface PackageArgEntry {
    index: number;
    arg: RegistryPackageArgView;
  }

  /** Auth-step probe mode. Drives the branch this component
   *  renders: `oauth` shows the scope selector + sign-in button, `static`
   *  shows the legacy token fields, `none` shows just the help text. */
  export type AuthProbeMode =
    | "idle"
    | "probing"
    | "none"
    | "static"
    | "oauth"
    | "probe_error";

  interface Props {
    envVars: RegistryEnvVarView[];
    enrichment: ConnectorEnrichmentView | null;
    values: Record<string, string>;
    onchange: (key: string, value: string) => void;
    /** Positional/named args the user must supply (registry value is null). */
    packageArgs?: PackageArgEntry[];
    /** Values typed by the user, keyed by the entry's `index`. */
    argValues?: Record<number, string>;
    onArgChange?: (index: number, value: string) => void;
    /** True when the remote URL points to a loopback host (127.0.0.1 / localhost
     *  / ::1). Triggers a dedicated "Local application required" card explaining
     *  that the MCP server is provided by a desktop app rather than a cloud
     *  service (Figma Dev Mode, future similar integrations). */
    localLoopback?: boolean;
    // ── OAuth probe props ──────────────────────────────────────────────
    /** Result of the auth-step probe (defaults to `"idle"` for legacy callers). */
    probeMode?: AuthProbeMode;
    /** Probe / discovery error message, surfaced under the form. */
    probeError?: string | null;
    /** OAuth discovery payload (scopes + AS URL) when `probeMode === "oauth"`. */
    oauthDiscovery?: McpOAuthDiscoveryResult | null;
    /** Scopes the user has selected in the multi-checkbox (subset of advertised). */
    oauthSelectedScopes?: string[];
    /** Identity returned by `mcp_oauth_login`, null until sign-in succeeds. */
    oauthAccount?: McpOAuthAccount | null;
    /** True while the OAuth browser dance is in flight. */
    oauthSigningIn?: boolean;
    /** Sign-in error message, surfaced inline. */
    oauthSigninError?: string | null;
    /** Toggle handler for a single scope checkbox. */
    onOAuthToggleScope?: (scope: string) => void;
    /** Click handler for the "Sign in to <provider>" button. */
    onOAuthSignin?: () => void;
    /** True when this connector requires a pre-registered OAuth client id
     *  (e.g. Figma) and none of {runtime env var, keychain, build-time}
     *  resolved one. Triggers the inline input field below the help text. */
    oauthClientIdMissing?: boolean;
    /** True when this connector uses a pre-registered OAuth app (override
     *  in effect). The scope selector is hidden in this case because scopes
     *  are configured at the provider's developer portal, not negotiated
     *  per sign-in - e.g. Figma rejects the PRM-published `mcp:connect`
     *  scope when sent to a third-party OAuth app's authorize endpoint. */
    oauthUsingPreRegisteredApp?: boolean;
    /** Name of the env var the connector expects, surfaced verbatim in
     *  the input field label so power users can also export it manually. */
    oauthClientIdEnvVar?: string | null;
    /** True while the saveClientId IPC is in flight. */
    oauthSavingClientId?: boolean;
    /** Error message from the saveClientId IPC. */
    oauthSaveClientIdError?: string | null;
    /** Click handler for the "Save" button - receives the trimmed value. */
    onOAuthSaveClientId?: (value: string) => void;
  }

  let {
    envVars,
    enrichment,
    values,
    onchange,
    packageArgs = [],
    argValues = {},
    onArgChange = () => {},
    localLoopback = false,
    probeMode = "idle",
    probeError = null,
    oauthDiscovery = null,
    oauthSelectedScopes = [],
    oauthAccount = null,
    oauthSigningIn = false,
    oauthSigninError = null,
    onOAuthToggleScope = () => {},
    onOAuthSignin = () => {},
    oauthClientIdMissing = false,
    oauthUsingPreRegisteredApp = false,
    oauthClientIdEnvVar = null,
    oauthSavingClientId = false,
    oauthSaveClientIdError = null,
    onOAuthSaveClientId = () => {},
  }: Props = $props();

  let clientIdDraft = $state("");

  // When there are no fields to fill *and* the connector isn't a local-loopback
  // one, the only message worth showing is the generic "no auth required".
  // Otherwise we hoist the help-text card and (when applicable) the
  // local-loopback explainer above the form so the user sees them even in the
  // "zero fields" case (the previous version hid them inside the `{:else}`).
  const hasFields = $derived(envVars.length > 0 || packageArgs.length > 0);
  const showHelpTextCard = $derived(
    Boolean(
      enrichment?.auth_help_i18n_key
        || enrichment?.auth_help_text
        || enrichment?.auth_help_url,
    ),
  );

  // OAuth mode helpers (Phase 5).
  const isProbing = $derived(probeMode === "probing");
  const isOAuthMode = $derived(probeMode === "oauth");
  // Resolved help text shown on the Auth step. Prefer the i18n key (so
  // connector-specific guidance is properly localised via svelte-i18n) and
  // fall back to the legacy bilingual `auth_help_text` baked into the
  // enrichments JSON, then to the generic default.
  const authHelpKey = $derived(enrichment?.auth_help_i18n_key ?? null);
  const authHelpInline = $derived(enrichment?.auth_help_text ?? null);
  // The legacy fields (catalog headers / package env) should only render when
  // the probe outcome calls for them - never alongside the OAuth flow, where
  // they'd compete with the "Sign in" button and confuse the operator.
  const showLegacyFields = $derived(
    hasFields && (probeMode === "static" || probeMode === "idle" || probeMode === "probe_error"),
  );
  const operatorLabel = $derived(enrichment?.operator_label ?? "");
  const oauthIdentityLabel = $derived(
    oauthAccount?.email ?? oauthAccount?.sub ?? null,
  );

  let revealed = $state<Record<string, boolean>>({});

  function toggleReveal(name: string): void {
    revealed = { ...revealed, [name]: !revealed[name] };
  }

  function inputType(envVar: RegistryEnvVarView): string {
    if (!envVar.isSecret) return "text";
    return revealed[envVar.name] ? "text" : "password";
  }
</script>

<div class="space-y-4" data-testid="wizard-step-auth">
  {#if localLoopback}
    <!-- Local-loopback MCPs (Figma Dev Mode, future similar) - the server is
         provided by an app installed on the user's machine, not a cloud
         endpoint. Surface this up front so "no auth required" doesn't read
         as "ready to connect" - the user still has work to do in the host
         app. -->
    <div
      class="flex gap-3 rounded-md border border-primary/30 bg-primary/5 px-4 py-3"
      data-testid="auth-local-loopback"
    >
      <Monitor size={18} class="mt-0.5 shrink-0 text-primary" aria-hidden="true" />
      <div class="space-y-1">
        <div class="flex items-center gap-2">
          <span class="text-sm font-medium text-foreground">
            {$t("integrations.wizard.local_loopback_title")}
          </span>
          <span class="rounded bg-primary/15 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-primary">
            {$t("integrations.wizard.local_loopback_badge")}
          </span>
        </div>
        <p class="text-sm text-muted-foreground leading-[1.5]">
          {$t("integrations.wizard.local_loopback_body")}
        </p>
      </div>
    </div>
  {/if}

  {#if showHelpTextCard}
    <!-- Connector-specific instructions (sourced from the catalog enrichment).
         Hoisted out of the form branch so it shows even when there are no
         fields to fill - that was the Figma blank-step bug. -->
    <div class="rounded-md border border-border bg-muted/40 px-4 py-3">
      {#if authHelpKey}
        <!-- i18n-resolved guidance (preferred path - properly localised).
             Split on \n\n into paragraphs and detect numbered steps so the
             template stays generic across all future connectors without
             requiring per-connector markup. -->
        {@const blocks = $t(authHelpKey)
          .split(/\n\n+/)
          .map((b) => b.trim())
          .filter((b) => b.length > 0)}
        <div class="space-y-2 text-sm text-muted-foreground" data-testid="auth-help-i18n">
          {#each blocks as block}
            {@const lines = block.split("\n").map((l) => l.trim()).filter(Boolean)}
            {@const allNumbered = lines.length > 0 && lines.every((l) => /^\d+\.\s/.test(l))}
            {#if allNumbered}
              <ol class="ml-5 list-decimal space-y-1">
                {#each lines as line}
                  <li>{line.replace(/^\d+\.\s*/, "")}</li>
                {/each}
              </ol>
            {:else}
              <p>{block}</p>
            {/if}
          {/each}
        </div>
      {:else if authHelpInline}
        <p class="text-sm text-muted-foreground">{authHelpInline}</p>
      {:else}
        <p class="text-sm text-muted-foreground">
          {$t("integrations.wizard.auth_help_default")}
        </p>
      {/if}
      {#if enrichment?.auth_help_url}
        <a
          href={enrichment.auth_help_url}
          target="_blank"
          rel="noopener noreferrer"
          onclick={handleExternalLinkClick}
          class="mt-1.5 inline-flex items-center gap-1 text-xs text-primary underline-offset-4 hover:underline"
          data-testid="auth-help-link"
        >
          <ExternalLink size={12} />
          {$t("integrations.wizard.auth_help_link")}
        </a>
      {/if}
    </div>
  {/if}

  {#if isProbing}
    <div class="flex items-center gap-2 rounded-md border border-border bg-muted/30 px-4 py-3" data-testid="auth-probing">
      <Spinner size={14} />
      <span class="text-sm text-muted-foreground">
        {$t("integrations.wizard.oauth_probing")}
      </span>
    </div>
  {/if}

  {#if isOAuthMode}
    <!-- OAuth mode. Renders the scope selector + sign-in
         affordance instead of asking the user to paste an Authorization
         Bearer value. The actual token is fetched, stored in the keychain,
         and rotated transparently by `apollia-auth::mcp_oauth_orchestrator`. -->
    <div class="space-y-4" data-testid="auth-oauth">
      <div class="flex gap-3 rounded-md border border-primary/30 bg-primary/5 px-4 py-3">
        <ShieldCheck size={18} class="mt-0.5 shrink-0 text-primary" aria-hidden="true" />
        <div class="space-y-1">
          <p class="text-sm font-medium text-foreground">
            {$t("integrations.wizard.oauth_required_title")}
          </p>
          <p class="text-sm text-muted-foreground leading-[1.5]">
            {$t("integrations.wizard.oauth_required_body")}
          </p>
        </div>
      </div>

      {#if oauthClientIdMissing}
        <!-- Provider exigeant un enregistrement OAuth (Figma). On affiche un
             champ de saisie pour que les users non-techniques n'aient PAS
             à passer par une variable d'environnement. Persisté en
             keychain via `mcp_oauth_store_client_id`. -->
        <div class="space-y-2 rounded-md border border-amber-500/40 bg-amber-500/5 px-4 py-3" data-testid="auth-oauth-client-id-input">
          <p class="text-sm font-medium text-foreground">
            {$t("integrations.wizard.oauth_client_id_title")}
          </p>
          <p class="text-[11.5px] text-muted-foreground leading-[1.5]">
            {$t("integrations.wizard.oauth_client_id_hint")}
          </p>
          <div class="flex gap-2">
            <Input
              type="text"
              value={clientIdDraft}
              oninput={(e) => (clientIdDraft = (e.currentTarget as HTMLInputElement).value)}
              placeholder={oauthClientIdEnvVar ?? "Client ID"}
              autocomplete="off"
              class="flex-1 font-mono text-xs"
              disabled={oauthSavingClientId}
              data-testid="auth-oauth-client-id-field"
            />
            <Button
              variant="primary-solid"
              size="sm"
              onclick={() => onOAuthSaveClientId(clientIdDraft)}
              disabled={oauthSavingClientId || clientIdDraft.trim().length === 0}
              data-testid="auth-oauth-client-id-save"
            >
              {#if oauthSavingClientId}
                <Spinner size={12} class="mr-1.5" />
              {/if}
              {$t("integrations.wizard.oauth_client_id_save")}
            </Button>
          </div>
          {#if oauthSaveClientIdError}
            <p class="text-[11px] text-destructive" data-testid="auth-oauth-client-id-error">
              {oauthSaveClientIdError}
            </p>
          {/if}
        </div>
      {/if}

      {#if oauthUsingPreRegisteredApp}
        <!-- Pre-registered OAuth app - scopes are baked at the provider's
             dev portal, not chooseable per sign-in. Surface that with a
             short note so the user doesn't expect the selector. -->
        <p class="text-[11.5px] text-muted-foreground leading-[1.5]" data-testid="auth-oauth-preregistered-scope-note">
          {$t("integrations.wizard.oauth_preregistered_scope_note")}
        </p>
      {:else if oauthDiscovery}
        <div class="space-y-2" data-testid="auth-oauth-scopes">
          <div class="space-y-1">
            <p class="text-sm font-medium text-foreground">
              {$t("integrations.wizard.oauth_scope_selector_title")}
            </p>
            <p class="text-[11.5px] text-muted-foreground leading-[1.5]">
              {oauthDiscovery.scopes_supported.length > 0
                ? $t("integrations.wizard.oauth_scope_selector_hint")
                : $t("integrations.wizard.oauth_scope_selector_empty")}
            </p>
          </div>
          {#if oauthDiscovery.scopes_supported.length > 0}
            <ul class="space-y-1 rounded-md border border-border bg-card px-3 py-2">
              {#each oauthDiscovery.scopes_supported as scope (scope)}
                {@const desc = oauthDiscovery.scope_descriptions[scope] ?? null}
                <li>
                  <label class="flex cursor-pointer items-start gap-2 py-1 text-sm">
                    <input
                      type="checkbox"
                      checked={oauthSelectedScopes.includes(scope)}
                      disabled={oauthSigningIn || oauthAccount !== null}
                      onchange={() => onOAuthToggleScope(scope)}
                      class="mt-0.5"
                      data-testid={`auth-oauth-scope-${scope}`}
                    />
                    <span class="flex-1">
                      <span class="font-mono text-[12.5px] text-foreground">{scope}</span>
                      {#if desc}
                        <span class="block text-[11px] text-muted-foreground">{desc}</span>
                      {/if}
                    </span>
                  </label>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}

      {#if oauthAccount}
        <div class="flex items-start gap-2 rounded-md border border-success/40 bg-success/5 px-4 py-3" data-testid="auth-oauth-success">
          <div class="flex-1 space-y-1">
            <p class="text-sm font-medium text-foreground">
              {oauthIdentityLabel
                ? $t("integrations.wizard.oauth_signin_success", {
                    values: { identity: oauthIdentityLabel },
                  })
                : $t("integrations.wizard.oauth_signin_success_anonymous")}
            </p>
          </div>
          <Button
            variant="outline"
            size="sm"
            onclick={onOAuthSignin}
            disabled={oauthSigningIn}
            data-testid="auth-oauth-reconnect-btn"
          >
            {$t("integrations.wizard.oauth_signin_reconnect")}
          </Button>
        </div>
      {:else}
        <Button
          variant="primary-solid"
          onclick={onOAuthSignin}
          disabled={oauthSigningIn}
          data-testid="auth-oauth-signin-btn"
        >
          {#snippet icon()}
            {#if oauthSigningIn}<Spinner size={14} />{:else}<LogIn size={14} />{/if}
          {/snippet}
          {#if oauthSigningIn}
            {$t("integrations.wizard.oauth_signin_in_progress")}
          {:else}
            {$t("integrations.wizard.oauth_signin_button", {
              values: { provider: operatorLabel || "MCP" },
            })}
          {/if}
        </Button>
      {/if}

      {#if oauthSigninError}
        <p class="text-sm text-destructive" data-testid="auth-oauth-error">
          {$t("integrations.wizard.oauth_signin_failed", {
            values: { error: oauthSigninError },
          })}
        </p>
      {/if}
    </div>
  {/if}

  {#if probeMode === "probe_error" && !isOAuthMode && !showLegacyFields}
    <p class="text-sm text-destructive leading-[1.5]" data-testid="auth-probe-error">
      {probeError ?? $t("integrations.wizard.oauth_discovery_failed")}
    </p>
  {/if}

  {#if !hasFields && !localLoopback && !showHelpTextCard && !isProbing && !isOAuthMode && probeMode !== "probe_error"}
    <p class="text-sm text-muted-foreground" data-testid="auth-no-vars">
      {$t("integrations.wizard.no_auth_required")}
    </p>
  {/if}

  {#if showLegacyFields}

    {#if packageArgs.length > 0}
      <div class="space-y-3" data-testid="wizard-step-args">
        {#each packageArgs as entry (entry.index)}
          {@const arg = entry.arg}
          {@const inputId = `arg-${entry.index}`}
          {@const label = arg.valueHint ?? `arg ${entry.index + 1}`}
          <FormField
            id={inputId}
            {label}
            labelClass="text-sm text-foreground"
            class="space-y-1.5"
            required={arg.isRequired}
            optional={!arg.isRequired}
            optionalLabel={`(${$t("integrations.wizard.optional")})`}
            hint={arg.description ?? undefined}
          >
            {#if arg.isRepeatable}
              <Textarea
                id={inputId}
                rows={2}
                class="w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm font-mono"
                placeholder={arg.valueHint ?? ""}
                value={argValues[entry.index] ?? ""}
                oninput={(e) =>
                  onArgChange(entry.index, (e.currentTarget as HTMLTextAreaElement).value)}
                data-testid={`arg-input-${entry.index}`}
              ></Textarea>
            {:else}
              <Input
                id={inputId}
                type="text"
                value={argValues[entry.index] ?? ""}
                oninput={(e) =>
                  onArgChange(entry.index, (e.currentTarget as HTMLInputElement).value)}
                placeholder={arg.valueHint ?? ""}
                autocomplete="off"
                data-testid={`arg-input-${entry.index}`}
              />
            {/if}
          </FormField>
        {/each}
      </div>
    {/if}

    {#if envVars.length === 0}
      <!-- args-only path: no env vars to render -->
    {:else}

    <div class="space-y-3">
      {#each envVars as envVar (envVar.name)}
        <FormField
          id={`env-${envVar.name}`}
          label={envVar.name}
          labelClass="text-sm text-foreground"
          class="space-y-1.5"
          required={envVar.isRequired}
          optional={!envVar.isRequired}
          optionalLabel={`(${$t("integrations.wizard.optional")})`}
          hint={envVar.description ?? undefined}
        >
          {#if envVar.isSecret}
            <p class="flex items-center gap-1 text-[11px] text-muted-foreground" data-testid={`env-encrypted-note-${envVar.name}`}>
              <Lock size={10} aria-hidden="true" />
              {$t("integrations.wizard.encrypted_locally")}
            </p>
          {/if}

          {#if envVar.isSecret}
            <Input
              id={`env-${envVar.name}`}
              type={inputType(envVar)}
              value={values[envVar.name] ?? ""}
              oninput={(e) => onchange(envVar.name, (e.currentTarget as HTMLInputElement).value)}
              placeholder="••••••••"
              autocomplete="off"
              data-testid={`env-input-${envVar.name}`}
            >
              {#snippet trailing()}
                <button
                  type="button"
                  class="text-muted-foreground transition-colors hover:text-foreground"
                  onclick={() => toggleReveal(envVar.name)}
                  aria-label={revealed[envVar.name]
                    ? $t("integrations.wizard.hide_value")
                    : $t("integrations.wizard.show_value")}
                  data-testid={`env-toggle-${envVar.name}`}
                >
                  {#if revealed[envVar.name]}
                    <EyeOff size={14} />
                  {:else}
                    <Eye size={14} />
                  {/if}
                </button>
              {/snippet}
            </Input>
          {:else}
            <Input
              id={`env-${envVar.name}`}
              type="text"
              value={values[envVar.name] ?? ""}
              oninput={(e) => onchange(envVar.name, (e.currentTarget as HTMLInputElement).value)}
              placeholder={envVar.name}
              autocomplete="off"
              data-testid={`env-input-${envVar.name}`}
            />
          {/if}
        </FormField>
      {/each}
    </div>
    {/if}
  {/if}
</div>
