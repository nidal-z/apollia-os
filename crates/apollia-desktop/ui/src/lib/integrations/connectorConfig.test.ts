import { describe, test, expect } from "vitest";
import { readFileSync } from "node:fs";
import {
  buildConnectorConfig,
  packageSpec,
  type ConnectorConfigInputs,
} from "$lib/integrations/connectorConfig";
import type { RegistryPackageView, RegistryServerView } from "$lib/types";

interface CatalogueEntry {
  package_identifier: string;
  package_runtime_hint?: string;
  package_version?: string;
  awaiting_registration?: boolean;
  withdrawn?: boolean;
}

const CATALOGUE: CatalogueEntry[] = JSON.parse(
  readFileSync(
    new URL("../../../../src/mcp/enrichments.json", import.meta.url),
    "utf8",
  ),
);

function npmPackage(identifier: string, version: string | null): RegistryPackageView {
  return {
    registryType: "npm",
    identifier,
    version,
    runtimeHint: "npx",
    transport: { type: "stdio" },
    environmentVariables: [],
    packageArguments: [],
  };
}

function inputs(pkg: RegistryPackageView): ConnectorConfigInputs {
  return {
    server: {
      name: pkg.identifier,
      title: "Memory",
      enrichment: null,
    } as unknown as RegistryServerView,
    connectionMode: "package",
    remote: null,
    pkg,
    oauthResolved: false,
    envValues: {},
    argValues: {},
    approvalLevel: "auto",
  };
}

describe("an npm connection is always asked at an exact version", () => {
  test("every listed npx entry of the selection launches its pinned version", () => {
    // GIVEN the listed npm entries of the embedded selection, as the desktop
    //       sends them with their pinned version
    const listed = CATALOGUE.filter(
      (e) =>
        e.package_runtime_hint === "npx" &&
        !e.awaiting_registration &&
        !e.withdrawn,
    );
    expect(listed.length).toBeGreaterThan(0);
    for (const entry of listed) {
      // WHEN the wizard builds the configuration it installs
      const config = buildConnectorConfig(
        inputs(npmPackage(entry.package_identifier, entry.package_version ?? null)),
        false,
      );
      // THEN the package is named at that exact version, never bare
      expect(entry.package_version).toMatch(/^\d+\.\d+\.\d+/);
      expect(config?.command).toBe("npx");
      expect(config?.args).toEqual([
        "-y",
        `${entry.package_identifier}@${entry.package_version}`,
      ]);
    }
  });

  test("a registry package keeps the version the registry gives", () => {
    // GIVEN a registry package published at 1.4.0
    const pkg = npmPackage("@someone/server", "1.4.0");
    // WHEN its package argument is built
    // THEN it names that version
    expect(packageSpec("npx", pkg)).toBe("@someone/server@1.4.0");
  });

  test("a package without a version is left for the desktop to resolve", () => {
    // GIVEN a package whose version is unknown, and a Python one
    const unknown = npmPackage("@someone/server", null);
    const blank = npmPackage("@someone/server", " ");
    // WHEN their package argument is built
    // THEN the identifier goes alone, and a uvx launcher never gets an npm suffix
    expect(packageSpec("npx", unknown)).toBe("@someone/server");
    expect(packageSpec("npx", blank)).toBe("@someone/server");
    expect(packageSpec("uvx", npmPackage("mcp-server-time", "2026.8.18"))).toBe(
      "mcp-server-time",
    );
  });
});
