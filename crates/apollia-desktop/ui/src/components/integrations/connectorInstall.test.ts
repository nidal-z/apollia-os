import { describe, test, expect } from "vitest";
import { readFileSync } from "node:fs";
import {
  deriveServerName,
  installConnector,
  sanitizeServerName,
} from "./ConnectorWizard.svelte";
import type { McpServerConfigInput } from "$lib/types";

// ─── The catalogue, read from the file the desktop binary embeds ───────────
//
// `crates/apollia-desktop/src/mcp/enrichments.json` is pulled in by
// `load_builtin_enrichments()` and surfaced to the wizard as
// `RegistryServerView.enrichment`, whose `operator_label` is always the `en`
// value (`crates/apollia-desktop/src/commands/mcp.rs:344`). A catalogue entry
// reaches the wizard with `name` = its registry identifier.

interface CatalogueEntry {
  package_identifier: string;
  registry_names: string[];
  operator_label: Record<string, string>;
}

const CATALOGUE: CatalogueEntry[] = JSON.parse(
  readFileSync(
    new URL("../../../../src/mcp/enrichments.json", import.meta.url),
    "utf8",
  ),
);

/** The catalogue entries a user can install from the registry: those that
 *  carry a registry name. Entries without one are not listed by the wizard. */
const REGISTRY_ENTRIES = CATALOGUE.flatMap((entry) =>
  entry.registry_names.map((registryName) => ({
    registryName,
    operatorLabel: entry.operator_label.en,
  })),
);

/** A `RegistryServerView` as the wizard receives it for a catalogue entry. */
function catalogueServer(registryName: string, operatorLabel: string) {
  return {
    name: registryName,
    title: null,
    enrichment: { operator_label: operatorLabel },
  } as unknown as Parameters<typeof deriveServerName>[0];
}

interface RecordedCall {
  cmd: string;
  args: Record<string, unknown>;
}

/** A stand-in for `@tauri-apps/api/core::invoke` that records what it got. */
function recordingInvoke(calls: RecordedCall[]) {
  return (async (cmd: string, args?: Record<string, unknown>) => {
    calls.push({ cmd, args: args ?? {} });
    return undefined;
  }) as Parameters<typeof installConnector>[0];
}

/** The keyring key `apollia-mcp::config::resolve_single_var` rebuilds at
 *  resolution time (`crates/apollia-mcp/src/config.rs:589`), and the one
 *  `SecretStore::key_for` wrote (`secret_store.rs:59-61`): `{name}:{var}`. */
function keyringKey(serverName: string, envVar: string): string {
  return `${serverName}:${envVar}`;
}

describe("connector install - one identifier for the secret and the config", () => {
  test("the catalogue replays with zero divergence", async () => {
    const divergent: string[] = [];

    for (const entry of REGISTRY_ENTRIES) {
      const server = catalogueServer(entry.registryName, entry.operatorLabel);
      const config = {
        name: deriveServerName(server),
        url: "https://example.invalid/mcp",
        transport: "streamable-http",
        env: { Authorization: "${APOLLIA_SECRET:Authorization}" },
        requires_approval: true,
        tags: [],
      } as unknown as McpServerConfigInput;

      const calls: RecordedCall[] = [];
      await installConnector(recordingInvoke(calls), config, [
        { envVar: "Authorization", value: "token-value" },
      ]);

      const stored = calls.filter((c) => c.cmd === "store_mcp_secret");
      const added = calls.filter((c) => c.cmd === "add_mcp_server");
      expect(stored).toHaveLength(1);
      expect(added).toHaveLength(1);

      const writtenUnder = stored[0].args.serverName as string;
      const installedAs = (added[0].args.config as McpServerConfigInput).name;
      if (writtenUnder !== installedAs) {
        divergent.push(`${entry.registryName} -> ${writtenUnder} != ${installedAs}`);
      }
    }

    // The catalogue replay the acceptance criterion reads. Run with
    // `npx vitest run --reporter=verbose` to see the line.
    console.log(`${divergent.length} / ${REGISTRY_ENTRIES.length} divergents`);
    expect(divergent).toEqual([]);
    expect(REGISTRY_ENTRIES).toHaveLength(9);
  });

  test("the key the engine rebuilds is the key the wizard wrote", async () => {
    for (const entry of REGISTRY_ENTRIES) {
      const server = catalogueServer(entry.registryName, entry.operatorLabel);
      const config = {
        name: deriveServerName(server),
        env: { API_KEY: "${APOLLIA_SECRET:API_KEY}" },
        transport: "stdio",
        requires_approval: true,
        tags: [],
      } as unknown as McpServerConfigInput;

      const calls: RecordedCall[] = [];
      await installConnector(recordingInvoke(calls), config, [
        { envVar: "API_KEY", value: "token-value" },
      ]);

      const stored = calls.find((c) => c.cmd === "store_mcp_secret");
      const written = keyringKey(
        stored?.args.serverName as string,
        stored?.args.envVar as string,
      );
      const rebuilt = keyringKey(config.name, "API_KEY");
      expect(written).toBe(rebuilt);
    }
  });

  test("no secret is filed under the registry identifier", async () => {
    for (const entry of REGISTRY_ENTRIES) {
      const server = catalogueServer(entry.registryName, entry.operatorLabel);
      const config = {
        name: deriveServerName(server),
        env: {},
        transport: "stdio",
        requires_approval: true,
        tags: [],
      } as unknown as McpServerConfigInput;

      const calls: RecordedCall[] = [];
      await installConnector(recordingInvoke(calls), config, [
        { envVar: "API_KEY", value: "token-value" },
      ]);

      const stored = calls.find((c) => c.cmd === "store_mcp_secret");
      expect(stored?.args.serverName).not.toBe(entry.registryName);
    }
  });

  test("an install without secrets still installs the configuration", async () => {
    const server = catalogueServer("com.notion/mcp", "Notion");
    const config = {
      name: deriveServerName(server),
      env: {},
      transport: "stdio",
      requires_approval: true,
      tags: [],
    } as unknown as McpServerConfigInput;

    const calls: RecordedCall[] = [];
    await installConnector(recordingInvoke(calls), config, []);

    expect(calls.map((c) => c.cmd)).toEqual(["add_mcp_server"]);
  });
});

describe("deriveServerName - the identifier the wizard settles on", () => {
  test("prefers the operator label over the registry identifier", () => {
    const server = catalogueServer("com.atlassian/rovo-mcp", "Atlassian (Jira + Confluence)");
    expect(deriveServerName(server)).toBe("atlassian-jira-confluence");
  });

  test("falls back to the registry identifier when the label is blank", () => {
    const server = catalogueServer("com.notion/mcp", "   ");
    expect(deriveServerName(server)).toBe("com-notion-mcp");
  });

  test("sanitises to what the backend validate_name accepts", () => {
    expect(sanitizeServerName("@modelcontextprotocol/server-filesystem")).toBe(
      "modelcontextprotocol-server-filesystem",
    );
    expect(sanitizeServerName("---")).toBe("mcp-server");
  });
});
