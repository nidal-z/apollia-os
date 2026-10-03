import { describe, test, expect, vi, beforeEach } from "vitest";

const ipc = vi.hoisted(() => vi.fn(async (_cmd: string, _args?: unknown): Promise<unknown> => null));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc }));

import type { ResolvedAgent } from "$lib/ipc/agentSheet";
import { exportAgent } from "$lib/ipc/agentExport";
import { canExport, exportFileName, publisherOf, shortFingerprint } from "./agentExport";

function agent(over: Partial<ResolvedAgent> = {}): ResolvedAgent {
  return {
    id: "veille",
    origin: "interface",
    package: null,
    version: "1",
    enabled: true,
    name: "Veille",
    mission: "Watch the market",
    examples: [],
    access: ["web_search"],
    sources: [],
    model: null,
    autonomy: "supervised",
    profile: null,
    overridden: [],
    ...over,
  };
}

describe("export of an agent", () => {
  beforeEach(() => ipc.mockClear());

  test("only an agent created in the interface offers the export", () => {
    // GIVEN an agent created in the interface, one from a package, and none
    // WHEN each is asked whether it can be exported
    // THEN only the first can
    expect(canExport(agent())).toBe(true);
    expect(canExport(agent({ origin: "package", package: "crm" }))).toBe(false);
    expect(canExport(null)).toBe(false);
  });

  test("the file name is the identifier and a semver version", () => {
    // GIVEN an agent numbered by the interface and one with a semver version
    // WHEN the offered file name is computed
    // THEN the first falls back to 1.0.0, as the runtime does
    expect(exportFileName(agent())).toBe("veille-1.0.0.apo");
    expect(exportFileName(agent({ version: "1.2.0" }))).toBe("veille-1.2.0.apo");
  });

  test("a blank publisher is no publisher, a typed one is trimmed", () => {
    // GIVEN a blank and a padded name
    // WHEN each is read
    // THEN the blank one leaves the setting in force
    expect(publisherOf("   ")).toBeNull();
    expect(publisherOf("  Ada ")).toBe("Ada");
  });

  test("a fingerprint is shown in groups of four", () => {
    // GIVEN a fingerprint of 64 hex characters, and none
    const fingerprint = "4f2a9c1e77b0d3a5" + "0".repeat(48);
    // WHEN it is shortened
    // THEN its first sixteen characters show in four groups
    expect(shortFingerprint(fingerprint)).toBe("4F2A 9C1E 77B0 D3A5");
    expect(shortFingerprint(null)).toBe("");
  });

  test("the export calls its command with the choices of the dialog", async () => {
    // GIVEN the shell answering a report
    ipc.mockResolvedValueOnce({ path: "/tmp/veille-1.0.0.apo", signed: true });

    // WHEN the sheet exports the agent signed under a name
    const report = await exportAgent("veille", true, "Ada", "/tmp/veille-1.0.0.apo");

    // THEN the command and its arguments are the ones the shell registers
    expect(ipc).toHaveBeenCalledWith("export_agent", {
      name: "veille",
      sign: true,
      publisher: "Ada",
      path: "/tmp/veille-1.0.0.apo",
    });
    expect(report.signed).toBe(true);
  });
});
