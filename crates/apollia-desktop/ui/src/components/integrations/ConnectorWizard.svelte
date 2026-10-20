<script lang="ts">
  /**
   * Adding a connection of the selection: two gestures, then the rest runs.
   *
   * The dialog opens on the one warning of the entry (the first gesture was
   * choosing it). Accepting it is the second; it is journaled by the desktop.
   * Only an authentication comes between, when the connection needs one (a
   * sign-in in the browser, or a key to type). The connection is then added,
   * Node.js downloaded first when it needs it, and tried on its own. A failed
   * trial leaves it added, with Retry and Remove; a refusal says who refused.
   */
  import { t } from "svelte-i18n";
  import { CheckCircle2, XCircle } from "lucide-svelte";
  import { Dialog } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { Badge } from "$lib/components/ui/badge";
  import { ProgressBar, Spinner } from "$lib/components/ui/progress";
  import {
    mcpOauthDiscover,
    mcpOauthLogin,
    mcpOauthResolveClientId,
    removeMcpServer,
    testMcpConnection,
    type PendingSecret,
  } from "$lib/ipc/connections";
  import WizardStepDisclaimer from "./WizardStepDisclaimer.svelte";
  import WizardStepAuth from "./WizardStepAuth.svelte";
  import WizardStepCoaching from "./WizardStepCoaching.svelte";
  import ConnectionRefusal from "./ConnectionRefusal.svelte";
  import GrantApolliaOption from "./GrantApolliaOption.svelte";
  import { grantConnectionToApollia } from "$lib/connections/apolliaGrant";
  import {
    buildConnectorConfig,
    deriveServerName,
    type ConnectorConfigInputs,
  } from "$lib/integrations/connectorConfig";
  import {
    addConnection,
    nodePercent,
    tryAddedConnection,
    type NodeRuntimeState,
    type Refusal,
  } from "$lib/integrations/addConnection";
  import { developerMode } from "$lib/stores/developerMode";
  import { navigateTo } from "$lib/stores/navigation";
  import type {
    McpOAuthAccount,
    McpOAuthDiscoveryResult,
    McpServerConfigInput,
    RegistryEnvVarView,
    RegistryServerView,
  } from "$lib/types";

  interface Props {
    server: RegistryServerView;
    open: boolean;
    onclose: () => void;
    /** Called once the connection exists, so the list shows it. */
    oncomplete: () => void;
    /** Called with a pre-filled prompt when the user clicks "Try" on an example. */
    ontryprompt?: (prompt: string) => void;
  }

  let {
    server,
    open,
    onclose,
    oncomplete,
    ontryprompt = () => {},
  }: Props = $props();

  /**
   * Where the addition stands. `warning` and `auth` wait for a gesture; the
   * others run on their own or report.
   */
  type Phase =
    | "warning"
    | "auth"
    | "preparing"
    | "trying"
    | "succeeded"
    | "trial_failed"
    | "refused"
    | "add_failed";

  let phase = $state<Phase>("warning");
  let envValues = $state<Record<string, string>>({});
  /** User-supplied values for package positional args, indexed by their
   *  position in `pkg.package_arguments`. Only consulted for args whose
   *  `value` is null (i.e. the user must supply it at install time). */
  let argValues = $state<Record<number, string>>({});
  /** The connection exists once added, whatever its trial answers. */
  let added = $state(false);
  // The agent of the chat is offered the connection, checked: the person
  // adding it expects to use it there. Unchecked, nothing is granted.
  let grantApollia = $state(true);
  let grantError = $state<string | null>(null);
  let nodeState = $state<NodeRuntimeState | null>(null);
  let refusal = $state<Refusal | null>(null);
  let addError = $state<string | null>(null);
  let trialTools = $state(0);
  let trialMessageKey = $state<string | null>(null);
  let trialDetail = $state<string | null>(null);
  let removing = $state(false);
  let removeError = $state<string | null>(null);

  // ── Auth probe + OAuth state ────────────────────────────
  // Once the warning is accepted on a `remote` connector, the server is probed
  // (no auth) once to detect whether it needs OAuth, a static token, or
  // nothing. The result decides whether the authentication step shows.
  //
  // `probeMode` lifecycle:
  //   "idle"        - not yet probed (initial state / probe disabled).
  //   "probing"     - request in flight.
  //   "none"        - server returned 200, no auth required (Figma local etc.).
  //   "static"      - server returned a non-OAuth 4xx, keep the token fields.
  //   "oauth"       - server returned 401 + OAuth discovery succeeded.
  //   "probe_error" - transport / discovery failed; the user can still try the
  //                   static-token path if the catalog declared headers.
  type ProbeMode = "idle" | "probing" | "none" | "static" | "oauth" | "probe_error";
  let probeMode = $state<ProbeMode>("idle");
  let probeError = $state<string | null>(null);
  let oauthWwwAuthenticate = $state<string | null>(null);
  let oauthDiscovery = $state<McpOAuthDiscoveryResult | null>(null);
  let oauthSelectedScopes = $state<string[]>([]);
  let oauthAccount = $state<McpOAuthAccount | null>(null);
  let oauthSigningIn = $state(false);
  let oauthSigninError = $state<string | null>(null);
  /** Pre-registered OAuth client id resolved from `oauth_pre_registered_client_id_env`.
   *  `null` when the catalog enrichment didn't declare one (CIMD/DCR path).
   *  `""` when declared but env var unset (surface a guidance error before
   *  even opening the browser). */
  let oauthClientIdOverride = $state<string | null>(null);

  $effect(() => {
    if (open) {
      phase = "warning";
      envValues = {};
      argValues = {};
      added = false;
      nodeState = null;
      refusal = null;
      addError = null;
      trialTools = 0;
      trialMessageKey = null;
      trialDetail = null;
      removing = false;
      removeError = null;
      probeMode = "idle";
      probeError = null;
      oauthWwwAuthenticate = null;
      oauthDiscovery = null;
      oauthSelectedScopes = [];
      oauthAccount = null;
      oauthSigningIn = false;
      oauthSigninError = null;
      oauthClientIdOverride = null;
    }
  });

  // ── Connection mode derivation (remote takes precedence over package) ───────
  const remote = $derived(server.remotes[0] ?? null);
  const pkg = $derived(server.packages?.[0] ?? null);
  const connectionMode = $derived<"remote" | "package" | null>(
    remote ? "remote" : pkg ? "package" : null,
  );

  const remoteHeadersAsEnvVars = $derived.by((): RegistryEnvVarView[] => {
    if (!remote) return [];
    return remote.headers.map((h) => ({
      name: h.name,
      description: h.description,
      isRequired: h.isRequired,
      isSecret: h.isSecret,
    }));
  });

  /** True when the remote MCP URL resolves to a loopback host (127.0.0.1,
   *  localhost, ::1). Used to surface the "Local application required" card
   *  for connectors whose server is hosted by a desktop app on the user's
   *  machine (Figma Dev Mode today, future similar integrations). */
  const isLocalLoopbackRemote = $derived.by((): boolean => {
    if (connectionMode !== "remote" || !remote) return false;
    try {
      const u = new URL(remote.url);
      return (
        u.hostname === "127.0.0.1" ||
        u.hostname === "localhost" ||
        u.hostname === "[::1]" ||
        u.hostname === "::1"
      );
    } catch {
      return false;
    }
  });

  const authEnvVars = $derived.by((): RegistryEnvVarView[] => {
    if (connectionMode === "remote") return remoteHeadersAsEnvVars;
    if (connectionMode === "package") return pkg?.environmentVariables ?? [];
    return [];
  });

  /** Positional args whose value is not pre-filled by the registry - the user
   *  must supply them at install time. Returned with their original index in
   *  `pkg.packageArguments` so `argValues[idx]` stays addressable. */
  const userPackageArgs = $derived.by((): { index: number; arg: import("$lib/types").RegistryPackageArgView }[] => {
    if (connectionMode !== "package" || !pkg) return [];
    return (pkg.packageArguments ?? [])
      .map((arg, index) => ({ index, arg }))
      .filter((entry) => entry.arg.value === null || entry.arg.value === undefined);
  });

  const argsComplete = $derived.by((): boolean => {
    return userPackageArgs.every(({ index, arg }) => {
      if (!arg.isRequired) return true;
      return (argValues[index] ?? "").trim().length > 0;
    });
  });

  // Auth-step completion gate. Branches mirror `probeMode` so the wizard
  // accepts the right kind of completion per server type.
  const authComplete = $derived.by((): boolean => {
    if (connectionMode === "remote") {
      switch (probeMode) {
        case "probing":
          return false;
        case "none":
          // Local-loopback / no-auth - user just needs to have read the
          // help text; advancing is always allowed once the probe returned.
          return true;
        case "oauth":
          // OAuth - must have completed the sign-in dance.
          return oauthAccount !== null;
        case "static":
        case "probe_error":
        case "idle": {
          // Fall back to the legacy required-field check so a user can still
          // paste a token manually if the probe failed but the catalog
          // declared headers.
          const required = authEnvVars.filter((v) => v.isRequired);
          return required.every((v) => (envValues[v.name] ?? "").trim() !== "");
        }
      }
    }
    // Package (stdio) mode keeps the original args + required-env check.
    const required = authEnvVars.filter((v) => v.isRequired);
    const envOk = required.every((v) => (envValues[v.name] ?? "").trim() !== "");
    return envOk && argsComplete;
  });

  const displayName = $derived(
    server.enrichment?.operator_label ?? server.title ?? server.name,
  );
  const serverName = $derived(deriveServerName(server));

  /** Whether the authentication step shows, once the probe (if any) answered. */
  function needsAuthStep(): boolean {
    if (connectionMode === "remote") {
      switch (probeMode) {
        case "oauth":
          return true;
        case "static":
        case "probe_error":
          return remoteHeadersAsEnvVars.some((v) => v.isRequired);
        default:
          return false;
      }
    }
    return (
      authEnvVars.some((v) => v.isRequired) ||
      userPackageArgs.some(({ arg }) => arg.isRequired)
    );
  }

  const configInputs = $derived<ConnectorConfigInputs>({
    server,
    connectionMode,
    remote,
    pkg,
    oauthResolved: probeMode === "oauth" && oauthAccount !== null,
    envValues,
    argValues,
  });

  const builtConfig = $derived(buildConnectorConfig(configInputs, false));

  /** The second gesture: the warning is accepted, the rest follows. */
  async function acceptWarning(): Promise<void> {
    if (connectionMode === "remote") {
      phase = "preparing";
      await runAuthProbe();
    }
    if (needsAuthStep()) {
      phase = "auth";
      return;
    }
    await add();
  }

  function handleEnvChange(key: string, value: string): void {
    envValues = { ...envValues, [key]: value };
  }

  function handleArgChange(index: number, value: string): void {
    argValues = { ...argValues, [index]: value };
  }

  // ── Probe + OAuth discovery ──────────────────────────────

  /** Build a no-auth probe config for the Auth step pre-flight. Only meaningful
   *  for remote connectors - package mode (stdio) needs its env to spawn at all,
   *  so we skip probing there and stay on the legacy field-based UX. */
  function buildProbeConfig(): McpServerConfigInput | null {
    if (connectionMode !== "remote" || !remote) return null;
    return {
      name: deriveServerName(server),
      url: remote.url,
      transport: remote.type,
      env: {},
      tags: [],
    };
  }

  /** Run a one-shot connection probe and route the wizard to the right Auth
   *  step branch. Run once, after the warning is accepted. */
  async function runAuthProbe(): Promise<void> {
    if (connectionMode !== "remote") {
      probeMode = "idle";
      return;
    }
    const probeConfig = buildProbeConfig();
    if (!probeConfig) {
      probeMode = "idle";
      return;
    }
    probeMode = "probing";
    probeError = null;
    try {
      const response = await testMcpConnection(probeConfig);
      if (response.kind === "success") {
        probeMode = "none";
      } else if (response.kind === "oauth_required") {
        oauthWwwAuthenticate = response.www_authenticate;
        await runOAuthDiscovery();
      } else {
        probeMode = "probe_error";
      }
    } catch (err: unknown) {
      // Probe transport errors - fall back to the legacy static-token UX so
      // the user can still try a manual token (catalog may declare headers).
      probeMode = remoteHeadersAsEnvVars.length > 0 ? "static" : "probe_error";
      probeError = err instanceof Error ? err.message : String(err);
    }
  }

  async function runOAuthDiscovery(): Promise<void> {
    if (!remote) return;
    try {
      const result = await mcpOauthDiscover(remote.url, oauthWwwAuthenticate);
      oauthDiscovery = result;

      // When the enrichment declares a pre-registered
      // OAuth client id env var, resolve it now so we can surface a
      // guidance error BEFORE the user clicks "Sign in" (saves a wasted
      // browser round-trip for AS that don't support CIMD/DCR - Figma).
      const envVar = server.enrichment?.oauth_pre_registered_client_id_env;
      if (envVar) {
        const resolved = await mcpOauthResolveClientId(envVar);
        oauthClientIdOverride = resolved ?? "";
      } else {
        oauthClientIdOverride = null;
      }

      // The AS offers neither DCR nor CIMD (Slack, GitHub) and no client id
      // is pre-registered: the browser flow cannot succeed. Use the static
      // token the catalog declares, or report the failure, rather than open
      // a browser onto an "unknown client" page.
      if (!result.registration_supported && !oauthClientIdOverride) {
        probeMode = remoteHeadersAsEnvVars.length > 0 ? "static" : "probe_error";
        return;
      }

      // Scope selection depends on the auth path:
      // - Generic CIMD/DCR (no override): PRM-published scopes are valid at
      //   the AS, default them all checked.
      // - Pre-registered override (Figma): scopes are fixed at the dev
      //   portal where the user created the OAuth app. Sending the PRM
      //   scopes (`mcp:connect` in Figma's case) to the standard OAuth
      //   authorize endpoint produces "Invalid scope" errors. Pass an
      //   empty list so the orchestrator omits `scope=` entirely; the AS
      //   then grants the scopes configured at registration time.
      const usingPreRegisteredApp =
        envVar !== null && envVar !== undefined && oauthClientIdOverride !== "";
      oauthSelectedScopes = usingPreRegisteredApp
        ? []
        : [...result.scopes_supported];

      probeMode = "oauth";
    } catch (err: unknown) {
      // Discovery failure: fall back to static-token UX if the catalog
      // declared headers, otherwise surface a clear error.
      probeMode = remoteHeadersAsEnvVars.length > 0 ? "static" : "probe_error";
      probeError = err instanceof Error ? err.message : String(err);
    }
  }

  function toggleOAuthScope(scope: string): void {
    if (oauthSelectedScopes.includes(scope)) {
      oauthSelectedScopes = oauthSelectedScopes.filter((s) => s !== scope);
    } else {
      oauthSelectedScopes = [...oauthSelectedScopes, scope];
    }
  }

  async function startOAuthSignin(): Promise<void> {
    if (!remote) return;
    // Guard: AS requires manual registration AND we have no client id → bail
    // with a clear guidance error rather than open the browser to a "client
    // doesn't exist" page (the Figma case before this fix).
    const envVar = server.enrichment?.oauth_pre_registered_client_id_env;
    if (envVar && (oauthClientIdOverride === null || oauthClientIdOverride === "")) {
      // Defensive - build-time injection (option_env!) should normally
      // populate this for end users. We only reach here in dev builds where
      // the build var wasn't set AND the operator hasn't exported the
      // runtime var either. The help_text card above documents the
      // registration procedure.
      oauthSigninError = $t("integrations.wizard.connector_not_configured");
      return;
    }
    oauthSigningIn = true;
    oauthSigninError = null;
    try {
      const account = await mcpOauthLogin({
        serverName: deriveServerName(server),
        serverUrl: remote.url,
        wwwAuthenticate: oauthWwwAuthenticate,
        scopes: oauthSelectedScopes,
        clientId: oauthClientIdOverride,
      });
      oauthAccount = account;
    } catch (err: unknown) {
      oauthSigninError = err instanceof Error ? err.message : String(err);
    } finally {
      oauthSigningIn = false;
    }
  }

  /** The secrets the operator typed, in the order the wizard collected them. */
  function pendingSecrets(): PendingSecret[] {
    const declared =
      connectionMode === "remote" && remote
        ? remote.headers.map((h) => ({ name: h.name, isSecret: h.isSecret }))
        : connectionMode === "package" && pkg
          ? (pkg.environmentVariables ?? []).map((v) => ({
              name: v.name,
              isSecret: v.isSecret,
            }))
          : [];
    const out: PendingSecret[] = [];
    for (const entry of declared) {
      const value = envValues[entry.name];
      if (entry.isSecret && value) {
        out.push({ envVar: entry.name, value });
      }
    }
    return out;
  }

  /** Add the connection, then try it. */
  async function add(): Promise<void> {
    const config = builtConfig;
    if (!config) {
      addError = $t("integrations.wizard.no_package_body");
      phase = "add_failed";
      return;
    }
    phase = "preparing";
    nodeState = null;
    addError = null;
    const outcome = await addConnection(config, pendingSecrets(), {
      onNode: (state) => (nodeState = state),
    });
    if (outcome.kind === "refused") {
      refusal = outcome.refusal;
      phase = "refused";
      return;
    }
    if (outcome.kind === "failed") {
      addError = outcome.message;
      phase = "add_failed";
      return;
    }
    added = true;
    await trial(config);
    if (grantApollia) await grantToApollia(config.name);
  }

  /** Give Apollia the tools of the connection; a failure leaves it added and says so. */
  async function grantToApollia(name: string): Promise<void> {
    grantError = null;
    try {
      await grantConnectionToApollia(name);
    } catch (err: unknown) {
      grantError = err instanceof Error ? err.message : String(err);
    }
  }

  /** Try the added connection; a failure leaves it added. */
  async function trial(config: McpServerConfigInput): Promise<void> {
    phase = "trying";
    const outcome = await tryAddedConnection(config.name);
    if (outcome.kind === "succeeded") {
      trialTools = outcome.tools;
      phase = "succeeded";
      return;
    }
    trialMessageKey = outcome.messageKey;
    trialDetail = outcome.detail;
    phase = "trial_failed";
  }

  async function retryTrial(): Promise<void> {
    if (builtConfig) await trial(builtConfig);
  }

  /** Remove the connection whose trial failed, then close. */
  async function removeAdded(): Promise<void> {
    removing = true;
    removeError = null;
    try {
      await removeMcpServer(serverName);
      added = false;
      oncomplete();
      onclose();
    } catch (err: unknown) {
      removeError = err instanceof Error ? err.message : String(err);
    } finally {
      removing = false;
    }
  }

  function handleRequestClose(): void {
    // A connection added, even one whose trial failed, shows in the list.
    if (added) oncomplete();
    onclose();
  }

  function openRefusingRule(): void {
    onclose();
    navigateTo("authorizations");
  }

  const nodeDownloading = $derived(nodeState?.state === "downloading");
  const nodeProgress = $derived(nodeState ? nodePercent(nodeState) : null);
  const busy = $derived(phase === "preparing" || phase === "trying" || removing);
</script>

<Dialog
  {open}
  onclose={handleRequestClose}
  size="lg"
  title={$t("integrations.wizard.title", { values: { name: displayName } })}
  data-testid="connector-wizard"
>
  <div class="min-h-[180px] space-y-4" data-phase={phase} data-testid="wizard-body">
    {#if phase === "warning"}
      <WizardStepDisclaimer name={displayName} />
      <GrantApolliaOption bind:checked={grantApollia} />
    {:else if phase === "auth"}
      <WizardStepAuth
        envVars={authEnvVars}
        enrichment={server.enrichment}
        values={envValues}
        onchange={handleEnvChange}
        packageArgs={userPackageArgs}
        argValues={argValues}
        onArgChange={handleArgChange}
        localLoopback={isLocalLoopbackRemote}
        probeMode={probeMode}
        probeError={probeError}
        oauthDiscovery={oauthDiscovery}
        oauthSelectedScopes={oauthSelectedScopes}
        oauthAccount={oauthAccount}
        oauthSigningIn={oauthSigningIn}
        oauthSigninError={oauthSigninError}
        oauthUsingPreRegisteredApp={
          (server.enrichment?.oauth_pre_registered_client_id_env ?? null) !== null
          && oauthClientIdOverride !== null
          && oauthClientIdOverride !== ""
        }
        onOAuthToggleScope={toggleOAuthScope}
        onOAuthSignin={startOAuthSignin}
      />
    {:else if phase === "preparing"}
      <div class="space-y-3" data-testid="wizard-preparing">
        <div class="flex items-center gap-2 text-sm text-foreground">
          <Spinner size={14} />
          {#if nodeDownloading}
            {$developerMode
              ? $t("dev.integrations.wizard.node_downloading")
              : $t("integrations.wizard.component_downloading")}
          {:else}
            {$t("integrations.wizard.preparing")}
          {/if}
        </div>
        {#if nodeDownloading}
          <div data-testid="wizard-node-progress">
            <ProgressBar value={nodeProgress ?? undefined} />
            <p class="mt-1 text-xs text-muted-foreground">
              {$t("integrations.wizard.component_verified")}
            </p>
          </div>
        {/if}
      </div>
    {:else if phase === "trying"}
      <div class="flex items-center gap-2 text-sm text-foreground" data-testid="wizard-trying">
        <Spinner size={14} />
        {$t("integrations.wizard.trying")}
      </div>
    {:else if phase === "succeeded"}
      {#if grantError}
        <p class="text-caption text-destructive" role="alert" data-testid="wizard-grant-error">
          {$t("connections.grant_apollia.failed", { values: { error: grantError } })}
        </p>
      {/if}
      <div
        class="rounded-md border border-success/30 bg-success/10 px-4 py-3"
        data-testid="test-result-success"
      >
        <div class="flex items-center gap-2">
          <CheckCircle2 size={16} class="shrink-0 text-success" aria-hidden="true" />
          <p class="text-sm font-medium text-success-a11y">
            {$t("integrations.wizard.added_success", { values: { count: trialTools } })}
          </p>
        </div>
        <p class="mt-1 text-xs text-muted-foreground">
          {$t("integrations.wizard.added_where")}
        </p>
      </div>
      <WizardStepCoaching
        serverName={server.name}
        serverTitle={server.title}
        ontry={(prompt) => ontryprompt(prompt)}
      />
    {:else if phase === "trial_failed"}
      <div
        class="space-y-2 rounded-md border border-destructive/30 bg-destructive/5 px-4 py-3"
        data-testid="test-result-error"
      >
        <div class="flex items-center gap-2">
          <XCircle size={16} class="shrink-0 text-destructive" aria-hidden="true" />
          <p class="text-sm font-medium text-destructive">
            {$t("integrations.wizard.added_failed")}
          </p>
          <Badge size="sm" variant="danger">{$t("integrations.wizard.failed_badge")}</Badge>
        </div>
        <p class="text-sm text-foreground" data-testid="test-result-error-message">
          {$t(trialMessageKey ?? "integrations.wizard.test_err.generic")}
        </p>
        {#if trialDetail}
          <details class="text-xs">
            <summary class="cursor-pointer text-muted-foreground">
              {$t("integrations.wizard.test_err.raw_details")}
            </summary>
            <p class="mt-1 break-all font-mono text-caption text-destructive/80">{trialDetail}</p>
          </details>
        {/if}
        {#if removeError}
          <p class="text-xs text-destructive" data-testid="wizard-remove-error">{removeError}</p>
        {/if}
      </div>
    {:else if phase === "refused" && refusal}
      <ConnectionRefusal {refusal} onopen={openRefusingRule} />
    {:else if phase === "add_failed"}
      <div
        class="space-y-2 rounded-md border border-destructive/30 bg-destructive/5 px-4 py-3"
        data-testid="wizard-finalize-error"
      >
        <div class="flex items-center gap-2">
          <XCircle size={16} class="shrink-0 text-destructive" aria-hidden="true" />
          <p class="text-sm font-medium text-destructive">
            {$t("integrations.wizard.add_failed")}
          </p>
        </div>
        {#if addError}
          <p class="break-all text-xs text-muted-foreground">{addError}</p>
        {/if}
      </div>
    {/if}
  </div>

  <div class="mt-6 flex items-center justify-end gap-2 border-t border-border pt-4">
    {#if phase === "warning"}
      <Button variant="outline" size="sm" onclick={handleRequestClose} data-testid="wizard-cancel-btn">
        {$t("common.cancel")}
      </Button>
      <Button
        variant="primary-solid"
        size="sm"
        onclick={() => void acceptWarning()}
        data-testid="wizard-warning-accept"
      >
        {$t("integrations.wizard.confirm_button")}
      </Button>
    {:else if phase === "auth"}
      <Button variant="outline" size="sm" onclick={handleRequestClose} data-testid="wizard-cancel-btn">
        {$t("common.cancel")}
      </Button>
      <Button
        variant="primary-solid"
        size="sm"
        onclick={() => void add()}
        disabled={!authComplete}
        data-testid="wizard-auth-continue"
      >
        {$t("integrations.wizard.confirm_button")}
      </Button>
    {:else if phase === "trial_failed"}
      <Button
        variant="destructive"
        size="sm"
        onclick={() => void removeAdded()}
        disabled={busy}
        data-testid="wizard-remove-btn"
      >
        {$t("integrations.wizard.remove")}
      </Button>
      <Button
        variant="outline"
        size="sm"
        onclick={() => void retryTrial()}
        disabled={busy}
        data-testid="test-retry-btn"
      >
        {$t("integrations.wizard.test_retry")}
      </Button>
      <Button variant="primary-solid" size="sm" onclick={handleRequestClose} data-testid="wizard-done-btn">
        {$t("common.close")}
      </Button>
    {:else if phase === "add_failed"}
      <Button variant="outline" size="sm" onclick={handleRequestClose} data-testid="wizard-cancel-btn">
        {$t("common.close")}
      </Button>
      <Button variant="primary-solid" size="sm" onclick={() => void add()} data-testid="wizard-retry-add-btn">
        {$t("integrations.wizard.test_retry")}
      </Button>
    {:else if phase === "succeeded" || phase === "refused"}
      <Button variant="primary-solid" size="sm" onclick={handleRequestClose} data-testid="wizard-done-btn">
        {$t("common.close")}
      </Button>
    {/if}
  </div>
</Dialog>
