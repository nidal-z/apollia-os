/**
 * What connecting an entry of the catalogue asks, once its warning is
 * accepted: a server reached at an address is probed once to learn whether it
 * wants a sign-in, a key or nothing; a server to download asks the values its
 * entry declares. Holds the values typed and the sign-in done, then builds the
 * configuration and the secrets to file.
 */
import { t } from "svelte-i18n";
import { get } from "svelte/store";
import {
  mcpOauthDiscover,
  mcpOauthLogin,
  mcpOauthResolveClientId,
  testMcpConnection,
  type PendingSecret,
} from "$lib/ipc/connections";
import {
  buildConnectorConfig,
  deriveServerName,
  type ConnectorConfigInputs,
} from "$lib/integrations/connectorConfig";
import type {
  McpOAuthAccount,
  McpOAuthDiscoveryResult,
  McpServerConfigInput,
  RegistryEnvVarView,
  RegistryPackageArgView,
  RegistryServerView,
} from "$lib/types";

/**
 * What the probe of a server reached at an address found: `none` needs
 * nothing, `static` a key the entry declares, `oauth` a sign-in in the
 * browser, `probe_error` an answer the flow could not read (the declared
 * fields stay offered).
 */
export type ProbeMode = "idle" | "probing" | "none" | "static" | "oauth" | "probe_error";

export interface PackageArgEntry {
  index: number;
  arg: RegistryPackageArgView;
}

function messageOf(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function isLoopback(url: string): boolean {
  try {
    const host = new URL(url).hostname;
    return host === "127.0.0.1" || host === "localhost" || host === "[::1]" || host === "::1";
  } catch {
    return false;
  }
}

export function createCatalogueSetup(server: RegistryServerView) {
  const remote = server.remotes[0] ?? null;
  const pkg = server.packages?.[0] ?? null;
  const connectionMode: "remote" | "package" | null = remote ? "remote" : pkg ? "package" : null;
  const remoteFields: RegistryEnvVarView[] = (remote?.headers ?? []).map((h) => ({
    name: h.name,
    description: h.description,
    isRequired: h.isRequired,
    isSecret: h.isSecret,
  }));
  const envVars: RegistryEnvVarView[] =
    connectionMode === "remote" ? remoteFields : connectionMode === "package" ? (pkg?.environmentVariables ?? []) : [];
  const packageArgs: PackageArgEntry[] =
    connectionMode === "package" && pkg
      ? (pkg.packageArguments ?? [])
          .map((arg, index) => ({ index, arg }))
          .filter((entry) => entry.arg.value === null || entry.arg.value === undefined)
      : [];
  const serverName = deriveServerName(server);
  const preRegisteredEnv = server.enrichment?.oauth_pre_registered_client_id_env ?? null;

  let envValues = $state<Record<string, string>>({});
  let argValues = $state<Record<number, string>>({});
  let probeMode = $state<ProbeMode>("idle");
  let probeError = $state<string | null>(null);
  let wwwAuthenticate: string | null = null;
  let discovery = $state<McpOAuthDiscoveryResult | null>(null);
  let scopes = $state<string[]>([]);
  let account = $state<McpOAuthAccount | null>(null);
  let signingIn = $state(false);
  let signinError = $state<string | null>(null);
  // The client id a pre-registered application resolves to: null when the
  // entry declares none, "" when it declares one that is not set.
  let clientIdOverride = $state<string | null>(null);

  function requiredFilled(): boolean {
    return envVars.filter((v) => v.isRequired).every((v) => (envValues[v.name] ?? "").trim() !== "");
  }

  function argsFilled(): boolean {
    return packageArgs.every(({ index, arg }) => !arg.isRequired || (argValues[index] ?? "").trim().length > 0);
  }

  async function discover(): Promise<void> {
    if (!remote) return;
    try {
      const result = await mcpOauthDiscover(remote.url, wwwAuthenticate);
      discovery = result;
      clientIdOverride = preRegisteredEnv ? ((await mcpOauthResolveClientId(preRegisteredEnv)) ?? "") : null;
      // Neither registration nor a pre-registered client: the browser flow
      // cannot succeed, so the declared key is asked instead.
      if (!result.registration_supported && !clientIdOverride) {
        probeMode = remoteFields.length > 0 ? "static" : "probe_error";
        return;
      }
      // A pre-registered application has its scopes fixed at its portal.
      scopes = preRegisteredEnv && clientIdOverride ? [] : [...result.scopes_supported];
      probeMode = "oauth";
    } catch (err: unknown) {
      probeMode = remoteFields.length > 0 ? "static" : "probe_error";
      probeError = messageOf(err);
    }
  }

  return {
    server,
    serverName,
    envVars,
    packageArgs,
    get localLoopback(): boolean {
      return connectionMode === "remote" && remote !== null && isLoopback(remote.url);
    },
    get envValues() {
      return envValues;
    },
    get argValues() {
      return argValues;
    },
    get probeMode() {
      return probeMode;
    },
    get probeError() {
      return probeError;
    },
    get discovery() {
      return discovery;
    },
    get scopes() {
      return scopes;
    },
    get account() {
      return account;
    },
    get signingIn() {
      return signingIn;
    },
    get signinError() {
      return signinError;
    },
    get usingPreRegisteredApp(): boolean {
      return preRegisteredEnv !== null && clientIdOverride !== null && clientIdOverride !== "";
    },
    setEnv(name: string, value: string): void {
      envValues = { ...envValues, [name]: value };
    },
    setArg(index: number, value: string): void {
      argValues = { ...argValues, [index]: value };
    },
    toggleScope(scope: string): void {
      scopes = scopes.includes(scope) ? scopes.filter((s) => s !== scope) : [...scopes, scope];
    },

    /** Probe a server reached at an address; a server to download is not probed. */
    async probe(): Promise<void> {
      if (connectionMode !== "remote" || !remote) return;
      probeMode = "probing";
      probeError = null;
      try {
        const response = await testMcpConnection({
          name: serverName,
          url: remote.url,
          transport: remote.type,
          env: {},
          tags: [],
        });
        if (response.kind === "success") {
          probeMode = "none";
        } else {
          wwwAuthenticate = response.www_authenticate;
          await discover();
        }
      } catch (err: unknown) {
        probeMode = remoteFields.length > 0 ? "static" : "probe_error";
        probeError = messageOf(err);
      }
    },

    /** Whether something is to be typed or signed in before the addition. */
    get needsInput(): boolean {
      if (connectionMode === "remote") {
        if (probeMode === "oauth") return true;
        if (probeMode === "static" || probeMode === "probe_error") return remoteFields.some((v) => v.isRequired);
        return false;
      }
      return envVars.some((v) => v.isRequired) || packageArgs.some(({ arg }) => arg.isRequired);
    },

    /** Whether what is asked is given. */
    get complete(): boolean {
      if (connectionMode === "remote") {
        if (probeMode === "probing") return false;
        if (probeMode === "none") return true;
        if (probeMode === "oauth") return account !== null;
        return requiredFilled();
      }
      return requiredFilled() && argsFilled();
    },

    async signIn(): Promise<void> {
      if (!remote) return;
      if (preRegisteredEnv && !clientIdOverride) {
        signinError = get(t)("integrations.wizard.connector_not_configured");
        return;
      }
      signingIn = true;
      signinError = null;
      try {
        account = await mcpOauthLogin({
          serverName,
          serverUrl: remote.url,
          wwwAuthenticate,
          scopes,
          clientId: clientIdOverride,
        });
      } catch (err: unknown) {
        signinError = messageOf(err);
      } finally {
        signingIn = false;
      }
    },

    /** The configuration to add, null when the entry has nothing to run. */
    config(): McpServerConfigInput | null {
      const inputs: ConnectorConfigInputs = {
        server,
        connectionMode,
        remote,
        pkg,
        oauthResolved: probeMode === "oauth" && account !== null,
        envValues,
        argValues,
      };
      return buildConnectorConfig(inputs, false);
    },

    /** The secrets typed, filed under the server's name before it is added. */
    secrets(): PendingSecret[] {
      const declared =
        connectionMode === "remote"
          ? remoteFields
          : connectionMode === "package"
            ? (pkg?.environmentVariables ?? [])
            : [];
      return declared
        .filter((entry) => entry.isSecret && envValues[entry.name])
        .map((entry) => ({ envVar: entry.name, value: envValues[entry.name] }));
    },
  };
}

export type CatalogueSetup = ReturnType<typeof createCatalogueSetup>;
