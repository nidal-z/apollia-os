<script module lang="ts">
  import type {
    McpServerConfigInput as McpServerConfigInputModule,
    RegistryServerView as RegistryServerViewModule,
  } from "$lib/types";

  /**
   * Backend validate_name() in apollia-mcp enforces `[a-z0-9_-]+` strictly,
   * so identifiers like `@modelcontextprotocol/server-filesystem` or
   * `com.figma/mcp-cloud` must be sanitised before being sent. We:
   * - lowercase the string
   * - replace every non-`[a-z0-9_-]` character with `-`
   * - collapse runs of `-` and trim them at the edges
   * - fall back to `mcp-server` if the result is empty.
   */
  export function sanitizeServerName(raw: string): string {
    const cleaned = raw
      .toLowerCase()
      .replace(/[^a-z0-9_-]+/g, "-")
      .replace(/-+/g, "-")
      .replace(/^-|-$/g, "");
    return cleaned.length > 0 ? cleaned : "mcp-server";
  }

  /**
   * The one identifier a catalogue connector is installed under.
   *
   * Prefer the operator label (e.g. "Local Files") over the registry
   * identifier (e.g. "@modelcontextprotocol/server-filesystem"). The operator
   * label is the human-facing card name; using it as the server name keeps the
   * installed-card label, the runtime logs, and the `mcp:<server>/<tool>`
   * invocation prefix consistent and readable.
   */
  export function deriveServerName(
    server: Pick<RegistryServerViewModule, "name" | "title" | "enrichment">,
  ): string {
    const label = server.enrichment?.operator_label ?? server.title ?? server.name;
    const source = label && label.trim().length > 0 ? label : server.name;
    return sanitizeServerName(source);
  }

  /** A secret typed in the wizard, waiting to be filed in the OS keyring. */
  export interface PendingSecret {
    envVar: string;
    value: string;
  }

  /** The subset of `@tauri-apps/api/core::invoke` the install sequence needs. */
  export type WizardInvoke = <T>(
    cmd: string,
    args?: Record<string, unknown>,
  ) => Promise<T>;

  /**
   * File the connector's secrets, then install its configuration.
   *
   * Both calls carry `config.name`, and that is the whole point of this
   * function existing. The keyring key is written as `{server_name}:{env_var}`
   * by `SecretStore::key_for` and rebuilt, at resolution time, from the name
   * carried by the *installed configuration*
   * (`apollia-mcp::config::resolve_env` passes `&self.name` to
   * `resolve_single_var`). Filing a secret under any other identifier, the
   * registry one included, hides it from the only lookup that will ever go
   * looking for it, and the connector answers `UnresolvedEnvVar` on first use.
   */
  export async function installConnector(
    invokeFn: WizardInvoke,
    config: McpServerConfigInputModule,
    secrets: readonly PendingSecret[],
  ): Promise<void> {
    for (const secret of secrets) {
      await invokeFn("store_mcp_secret", {
        serverName: config.name,
        envVar: secret.envVar,
        value: secret.value,
      });
    }
    await invokeFn("add_mcp_server", { config });
  }
</script>

<script lang="ts">
  import { t } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import {
    BookOpen,
    KeyRound,
    PlugZap,
    Sparkles,
  } from "lucide-svelte";
  import { Dialog } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { Stepper } from "$lib/components/ui/stepper";
  import WizardStepDisclaimer, {
    DISCLAIMER_ITEMS,
    isDisclaimerVersionAccepted,
    recordDisclaimerAcceptance,
  } from "./WizardStepDisclaimer.svelte";
  import WizardStepAuth from "./WizardStepAuth.svelte";
  import WizardStepTest from "./WizardStepTest.svelte";
  import WizardStepCoaching from "./WizardStepCoaching.svelte";
  import type {
    McpConnectionTestResponse,
    McpOAuthAccount,
    McpOAuthDiscoveryResult,
    McpServerConfigInput,
    RegistryEnvVarView,
    RegistryServerView,
  } from "$lib/types";

  interface Props {
    server: RegistryServerView;
    open: boolean;
    /** When true, expose "bypass test" escape hatch + skip disclaimer re-prompt logic. */
    builder?: boolean;
    onclose: () => void;
    oncomplete: () => void;
    /** Called with a pre-filled prompt when the user clicks "Try" on a coaching card. */
    ontryprompt?: (prompt: string) => void;
  }

  let {
    server,
    open,
    builder = false,
    onclose,
    oncomplete,
    ontryprompt = () => {},
  }: Props = $props();

  type WizardStep = "disclaimer" | "auth" | "test" | "coaching";

  // ── Disclaimer state (step 1) ───────────────────────────────────────────────
  let disclaimerChecks = $state<Record<string, boolean>>({});
  let disclaimerVersionOk = $state(false);

  const disclaimerComplete = $derived(
    DISCLAIMER_ITEMS.every((key) => {
      const short = key.replace("i18n:integrations.disclaimer.items.", "");
      return disclaimerChecks[short] === true;
    }),
  );

  // ── Auth / Test / Coaching state ────────────────────────────────────────────
  let currentStepIndex = $state(0);
  let envValues = $state<Record<string, string>>({});
  /** User-supplied values for package positional args, indexed by their
   *  position in `pkg.package_arguments`. Only consulted for args whose
   *  `value` is null (i.e. the user must supply it at install time). */
  let argValues = $state<Record<number, string>>({});
  let approvalLevel = $state<"auto" | "ask" | "readonly">("ask");
  let testSucceeded = $state(false);
  let testBypassed = $state(false);
  let finalizing = $state(false);
  let finalizeError = $state<string | null>(null);

  // ── Auth-step probe + OAuth state ────────────────────────
  // When the user enters the Auth step on a `remote` connector, we probe the
  // server (no auth) once to detect whether it needs OAuth, a static token,
  // or nothing. The result drives which UI branch the step renders.
  //
  // `probeMode` lifecycle:
  //   "idle"        - not yet probed (initial state / probe disabled).
  //   "probing"     - request in flight, render a spinner.
  //   "none"        - server returned 200, no auth required (Figma local etc.).
  //   "static"      - server returned a non-OAuth 4xx, keep legacy token UX.
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
  let oauthSavingClientId = $state(false);
  let oauthSaveClientIdError = $state<string | null>(null);

  $effect(() => {
    if (open) {
      currentStepIndex = 0;
      envValues = {};
      argValues = {};
      approvalLevel = "ask";
      testSucceeded = false;
      testBypassed = false;
      finalizeError = null;
      probeMode = "idle";
      probeError = null;
      oauthWwwAuthenticate = null;
      oauthDiscovery = null;
      oauthSelectedScopes = [];
      oauthAccount = null;
      oauthSigningIn = false;
      oauthSigninError = null;
      oauthClientIdOverride = null;
      oauthSavingClientId = false;
      oauthSaveClientIdError = null;
      // Pre-check disclaimer if current version is already accepted.
      void isDisclaimerVersionAccepted().then((v) => {
        disclaimerVersionOk = v;
        if (v) {
          disclaimerChecks = {
            code_on_machine: true,
            external_data: true,
            revocable: true,
            read_capabilities: true,
          };
        } else {
          disclaimerChecks = {};
        }
      });
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

  const hasWriteTools = $derived.by((): boolean => {
    // Heuristic fallback until backend exposes `enrichment.capabilities.has_write`:
    // assume a server has write tools unless it is an explicit read-only category.
    const ro = new Set(["search", "analytics"]);
    const cat = server.enrichment?.category ?? server.category ?? "";
    return !ro.has(cat);
  });

  const canInstall = $derived(connectionMode !== null);

  // ── Ordered wizard steps ────────────────────────────────────────────────────
  const steps: readonly WizardStep[] = [
    "disclaimer",
    "auth",
    "test",
    "coaching",
  ] as const;

  const stepperSteps = $derived([
    { label: $t("integrations.wizard.step_disclaimer"), icon: BookOpen },
    { label: $t("integrations.wizard.step_auth"), icon: KeyRound },
    { label: $t("integrations.wizard.step_test"), icon: PlugZap },
    { label: $t("integrations.wizard.step_coaching"), icon: Sparkles },
  ]);

  const currentStep = $derived(steps[currentStepIndex]);
  const totalSteps = steps.length;

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

  const canAdvance = $derived.by((): boolean => {
    switch (currentStep) {
      case "disclaimer":
        return disclaimerComplete;
      case "auth":
        return authComplete;
      case "test":
        return testSucceeded || testBypassed;
      case "coaching":
        return canInstall && !finalizing;
      default:
        return false;
    }
  });

  // ── Stdio launcher resolution ──────────────────────────────────────────────
  // Map (registry_type, runtime_hint) to the actual launcher invocation. The
  // registry's `runtime_hint` field is semantically informational ("which
  // runtime is needed", e.g. `node`, `python`) - not an executable name. The
  // wizard previously passed it straight to `command`, which produced
  // `command="node", args=["@scope/pkg", ...]` for every npm-based server and
  // made Node interpret the package name as a relative script path.
  //
  // This helper centralises the mapping so all 18 catalog entries plus any
  // future runtime end up with a correct, non-interactive launcher invocation.
  function resolveStdioLauncher(
    registryType: string | null | undefined,
    runtimeHint: string | null | undefined,
  ): { command: string; prefixArgs: string[] } {
    const hint = (runtimeHint ?? "").toLowerCase();
    const reg = (registryType ?? "").toLowerCase();

    // Direct launcher commands declared explicitly by the registry entry.
    if (hint === "npx") return { command: "npx", prefixArgs: ["-y"] };
    if (hint === "uvx") return { command: "uvx", prefixArgs: [] };
    if (hint === "bunx") return { command: "bunx", prefixArgs: [] };

    // Runtime-name hints - map to the conventional launcher for that ecosystem.
    if (hint === "node" || reg === "npm") {
      return { command: "npx", prefixArgs: ["-y"] };
    }
    if (hint === "python" || reg === "pypi") {
      return { command: "uvx", prefixArgs: [] };
    }

    // Unknown hint - surface it verbatim so the user can fix it in custom mode
    // rather than silently rewriting to something potentially wrong.
    return { command: runtimeHint || "npx", prefixArgs: [] };
  }

  // ── Config builder ──────────────────────────────────────────────────────────
  function buildConfig(forTest: boolean): McpServerConfigInput | null {
    const safeName = deriveServerName(server);
    if (connectionMode === "remote" && remote) {
      const env: Record<string, string> = {};
      if (probeMode === "oauth" && oauthAccount) {
        // OAuth flow completed (Phase 5). The token lives in the keyring under
        // `mcp_oauth:{server_name}`; the transport injects it dynamically at
        // each request via `${APOLLIA_OAUTH}` (resolved + Bearer-prefixed by
        // `apollia-mcp::config::resolve_single_var`).
        //
        // Catalog headers other than `Authorization` (rare) are still passed
        // through as static placeholders so a server that requires both a
        // Bearer + an extra static header keeps working.
        env["Authorization"] = "${APOLLIA_OAUTH}";
        for (const header of remote.headers) {
          if (header.name.toLowerCase() === "authorization") continue;
          env[header.name] =
            header.isSecret && !forTest
              ? `\${APOLLIA_SECRET:${header.name}}`
              : (envValues[header.name] ?? "");
        }
      } else {
        // Legacy / static-token path - operator pasted a value into the form.
        for (const header of remote.headers) {
          env[header.name] =
            header.isSecret && !forTest
              ? `\${APOLLIA_SECRET:${header.name}}`
              : (envValues[header.name] ?? "");
        }
      }
      return {
        name: safeName,
        url: remote.url,
        transport: remote.type,
        env,
        requires_approval: approvalLevel === "ask",
        tags: [],
      };
    }
    if (connectionMode === "package" && pkg) {
      const env: Record<string, string> = {};
      for (const envVar of pkg.environmentVariables ?? []) {
        env[envVar.name] =
          envVar.isSecret && !forTest
            ? `\${APOLLIA_SECRET:${envVar.name}}`
            : (envValues[envVar.name] ?? "");
      }
      // Expand declared package arguments into a flat argv. Each entry either
      // contributes its registry-fixed `value` verbatim, or pulls the user's
      // input from `argValues` (split on whitespace for `isRepeatable` args,
      // which is how `@modelcontextprotocol/server-filesystem` declares its
      // allowed-directory list).
      const extraArgs: string[] = (pkg.packageArguments ?? []).flatMap(
        (arg, idx) => {
          if (arg.value !== null && arg.value !== undefined) return [arg.value];
          const raw = (argValues[idx] ?? "").trim();
          if (raw.length === 0) return [];
          return arg.isRepeatable ? raw.split(/\s+/).filter(Boolean) : [raw];
        },
      );
      const launcher = resolveStdioLauncher(pkg.registryType, pkg.runtimeHint);
      return {
        name: safeName,
        command: launcher.command,
        args: [...launcher.prefixArgs, pkg.identifier, ...extraArgs],
        env,
        transport: pkg.transport?.type ?? "stdio",
        requires_approval: approvalLevel === "ask",
        tags: [],
      };
    }
    return null;
  }

  const testConfig = $derived(buildConfig(true));
  const builtConfig = $derived(buildConfig(false));

  // ── Navigation ──────────────────────────────────────────────────────────────
  async function goNext(): Promise<void> {
    if (!canAdvance) return;
    if (currentStep === "disclaimer" && !disclaimerVersionOk) {
      await recordDisclaimerAcceptance();
      disclaimerVersionOk = true;
    }
    if (currentStepIndex < totalSteps - 1) currentStepIndex += 1;
  }

  function goBack(): void {
    if (currentStepIndex > 0) currentStepIndex -= 1;
  }

  function handleDisclaimerChange(key: string, value: boolean): void {
    disclaimerChecks = { ...disclaimerChecks, [key]: value };
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
      requires_approval: false,
      tags: [],
    };
  }

  /** Run a one-shot connection probe and route the wizard to the right Auth
   *  step branch. Triggered once when the user reaches the Auth step. */
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
      const response = await invoke<McpConnectionTestResponse>(
        "test_mcp_connection",
        { config: probeConfig },
      );
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
      const result = await invoke<McpOAuthDiscoveryResult>("mcp_oauth_discover", {
        url: remote.url,
        wwwAuthenticate: oauthWwwAuthenticate,
      });
      oauthDiscovery = result;

      // When the enrichment declares a pre-registered
      // OAuth client id env var, resolve it now so we can surface a
      // guidance error BEFORE the user clicks "Sign in" (saves a wasted
      // browser round-trip for AS that don't support CIMD/DCR - Figma).
      const envVar = server.enrichment?.oauth_pre_registered_client_id_env;
      if (envVar) {
        const resolved = await invoke<string | null>(
          "mcp_oauth_resolve_client_id",
          { envVar },
        );
        oauthClientIdOverride = resolved ?? "";
      } else {
        oauthClientIdOverride = null;
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

  /** Persist a user-entered OAuth client id to the keychain via IPC.
   *  Triggered from the input field shown in the Auth step when the
   *  connector's enrichment declares `oauth_pre_registered_client_id_env`
   *  and none of {runtime env var, prior keychain entry, build-time const}
   *  resolved a value. Subsequent calls to `mcp_oauth_resolve_client_id`
   *  return this value transparently (priority 2). */
  async function saveOAuthClientId(value: string): Promise<void> {
    const envVar = server.enrichment?.oauth_pre_registered_client_id_env;
    if (!envVar) return;
    const trimmed = value.trim();
    if (!trimmed) {
      oauthSaveClientIdError = "Le Client ID est vide.";
      return;
    }
    oauthSavingClientId = true;
    oauthSaveClientIdError = null;
    try {
      await invoke("mcp_oauth_store_client_id", {
        envVar,
        value: trimmed,
      });
      oauthClientIdOverride = trimmed;
      // Mirror runOAuthDiscovery's pre-registered branch: once the user
      // supplies a pre-registered client_id, scopes are baked at the
      // provider's dev portal - sending PRM-published placeholders
      // (`mcp:connect` on Figma) to the AS produces "Invalid scope".
      // Empty list ⇒ orchestrator omits `scope=` from the authorize URL.
      oauthSelectedScopes = [];
    } catch (err: unknown) {
      oauthSaveClientIdError = err instanceof Error ? err.message : String(err);
    } finally {
      oauthSavingClientId = false;
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
      oauthSigninError = `Ce connecteur n'est pas encore configuré dans ce build d'Apollia. Voir l'aide ci-dessus.`;
      return;
    }
    oauthSigningIn = true;
    oauthSigninError = null;
    try {
      const account = await invoke<McpOAuthAccount>("mcp_oauth_login", {
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

  // Trigger the probe automatically when the user reaches the Auth step.
  // Runs once per opening; the probe state is reset in the `$effect(open)` block.
  $effect(() => {
    if (open && currentStep === "auth" && probeMode === "idle") {
      void runAuthProbe();
    }
  });

  function handleTestSuccess(): void {
    testSucceeded = true;
  }

  function handleFixAuth(): void {
    testSucceeded = false;
    testBypassed = false;
    currentStepIndex = steps.indexOf("auth");
  }

  function handleBypass(): void {
    if (!builder) return;
    testBypassed = true;
  }

  function handleRequestClose(): void {
    if (
      currentStepIndex > 0 &&
      currentStep !== "coaching" &&
      !confirm($t("integrations.wizard.confirm_exit"))
    ) {
      return;
    }
    onclose();
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

  async function finalize(): Promise<void> {
    if (!builtConfig) return;
    finalizing = true;
    finalizeError = null;
    try {
      await installConnector(invoke, builtConfig, pendingSecrets());
      oncomplete();
      // The first-connection walkthrough moved out of the wizard: it used to run
      // while this dialog was closed, pointing at a sheet that was never open.
      // It is now the "connect" micro-tour, launched from Getting started.
      onclose();
    } catch (err: unknown) {
      finalizeError = err instanceof Error ? err.message : String(err);
    } finally {
      finalizing = false;
    }
  }

</script>

<Dialog
  {open}
  onclose={handleRequestClose}
  size="lg"
  title={$t("integrations.wizard.title", {
    values: { name: server.title ?? server.name },
  })}
  data-testid="connector-wizard"
>
  <!-- Stepper -->
  <div class="mb-6">
    <Stepper steps={stepperSteps} current={currentStepIndex} />
  </div>

  <!-- Step content -->
  <div class="min-h-[220px]">
    {#if currentStep === "disclaimer"}
      <WizardStepDisclaimer
        checks={disclaimerChecks}
        onchange={handleDisclaimerChange}
      />
    {:else if currentStep === "auth"}
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
        oauthClientIdMissing={
          (server.enrichment?.oauth_pre_registered_client_id_env ?? null) !== null
          && (oauthClientIdOverride === null || oauthClientIdOverride === "")
        }
        oauthUsingPreRegisteredApp={
          (server.enrichment?.oauth_pre_registered_client_id_env ?? null) !== null
          && oauthClientIdOverride !== null
          && oauthClientIdOverride !== ""
        }
        oauthClientIdEnvVar={server.enrichment?.oauth_pre_registered_client_id_env ?? null}
        oauthSavingClientId={oauthSavingClientId}
        oauthSaveClientIdError={oauthSaveClientIdError}
        onOAuthSaveClientId={saveOAuthClientId}
        onOAuthToggleScope={toggleOAuthScope}
        onOAuthSignin={startOAuthSignin}
      />
    {:else if currentStep === "test"}
      {#if testConfig}
        <WizardStepTest
          config={testConfig}
          allowBypass={builder}
          onsuccess={handleTestSuccess}
          onfixauth={handleFixAuth}
          onbypass={handleBypass}
        />
      {:else}
        <div
          class="space-y-2 rounded-md border border-border bg-muted/40 p-4"
          data-testid="wizard-step-test-unavailable"
        >
          <p class="text-sm text-muted-foreground">
            {$t("integrations.wizard.test_unavailable")}
          </p>
        </div>
      {/if}
    {:else if currentStep === "coaching"}
      {#if canInstall}
        <WizardStepCoaching
          serverName={server.name}
          serverTitle={server.title}
          {hasWriteTools}
          {approvalLevel}
          onchange={(l) => (approvalLevel = l)}
          ontry={ontryprompt}
        />
      {:else}
        <div
          class="space-y-3 rounded-md border border-border bg-muted/40 p-4"
          data-testid="wizard-no-package"
        >
          <p class="text-sm font-medium text-foreground">
            {$t("integrations.wizard.no_package_title")}
          </p>
          <p class="text-sm text-muted-foreground">
            {$t("integrations.wizard.no_package_body")}
          </p>
        </div>
      {/if}
    {/if}
  </div>

  {#if finalizeError}
    <p
      class="mt-3 text-sm text-destructive"
      data-testid="wizard-finalize-error"
    >
      {finalizeError}
    </p>
  {/if}

  <!-- Navigation bar -->
  <div
    class="mt-6 flex items-center justify-between border-t border-border pt-4 sticky bottom-0 bg-background"
  >
    <Button
      variant="outline"
      size="sm"
      onclick={goBack}
      disabled={currentStepIndex === 0}
      data-testid="wizard-back-btn"
    >
      {$t("common.back")}
    </Button>

    {#if currentStep !== "coaching"}
      <Button
        variant="primary-solid"
        size="sm"
        onclick={() => void goNext()}
        disabled={!canAdvance}
        data-testid="wizard-next-btn"
      >
        {$t("integrations.wizard.next")}
      </Button>
    {:else}
      <Button
        variant="primary-gradient"
        size="sm"
        onclick={() => void finalize()}
        disabled={finalizing || !canInstall}
        data-testid="wizard-confirm-btn"
      >
        {finalizing
          ? $t("integrations.wizard.adding")
          : $t("integrations.wizard.confirm_button")}
      </Button>
    {/if}
  </div>
</Dialog>
