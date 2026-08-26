// Naming and configuration rules of the connector wizard.
//
// These four functions decide what gets sent to `install_connector` and to
// `test_mcp_connection`. They are pure: every input is an argument, nothing
// reads component state, so they stay node-testable and the wizard component
// keeps only the interaction.

import type {
  McpServerConfigInput,
  RegistryPackageView,
  RegistryRemoteView,
  RegistryServerView,
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
  server: Pick<RegistryServerView, "name" | "title" | "enrichment">,
): string {
  const label = server.enrichment?.operator_label ?? server.title ?? server.name;
  const source = label && label.trim().length > 0 ? label : server.name;
  return sanitizeServerName(source);
}

/**
 * Map (registry_type, runtime_hint) to the actual launcher invocation. The
 * registry's `runtime_hint` field is semantically informational ("which
 * runtime is needed", e.g. `node`, `python`) - not an executable name. The
 * wizard previously passed it straight to `command`, which produced
 * `command="node", args=["@scope/pkg", ...]` for every npm-based server and
 * made Node interpret the package name as a relative script path.
 *
 * This helper centralises the mapping so all 18 catalog entries plus any
 * future runtime end up with a correct, non-interactive launcher invocation.
 */
export function resolveStdioLauncher(
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

/** Everything `buildConnectorConfig` reads, gathered by the wizard component. */
export interface ConnectorConfigInputs {
  server: RegistryServerView;
  connectionMode: "remote" | "package" | null;
  remote: RegistryRemoteView | null;
  pkg: RegistryPackageView | null;
  /** `true` when the OAuth flow completed and a token sits in the keyring. */
  oauthResolved: boolean;
  envValues: Record<string, string>;
  argValues: Record<number, string>;
  approvalLevel: "auto" | "ask" | "readonly";
}

/**
 * The configuration the wizard installs, or tests.
 *
 * `forTest` is the one difference between the two: a test connection needs the
 * literal secret the operator just typed, while an install stores a
 * `${APOLLIA_SECRET:...}` placeholder the transport resolves at each request.
 */
export function buildConnectorConfig(
  inputs: ConnectorConfigInputs,
  forTest: boolean,
): McpServerConfigInput | null {
  const {
    server,
    connectionMode,
    remote,
    pkg,
    oauthResolved,
    envValues,
    argValues,
    approvalLevel,
  } = inputs;
  const safeName = deriveServerName(server);
  if (connectionMode === "remote" && remote) {
    const env: Record<string, string> = {};
    if (oauthResolved) {
      // OAuth flow completed. The token lives in the keyring under
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
