import { describe, test, expect, vi } from "vitest";
import { get } from "svelte/store";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

import {
  findSelectionEntry,
  pendingConnectionAdd,
  requestConnectionAdd,
  takePendingConnectionAdd,
} from "$lib/stores/connectionAdd";
import { currentRoute } from "$lib/stores/navigation";
import type { RegistryServerView } from "$lib/types";

function entry(name: string, pkg: string | null = null): RegistryServerView {
  return {
    name,
    title: null,
    description: null,
    version: "",
    repository_url: null,
    website_url: null,
    packages: pkg
      ? [{ registryType: "npm", identifier: pkg, version: "1.0.0", runtimeHint: "npx", transport: { type: "stdio" }, environmentVariables: [] }]
      : null,
    trust_level: "official",
    category: null,
    enrichment: null,
    is_installed: false,
    remotes: [],
  } as unknown as RegistryServerView;
}

describe("opening the add dialog on one entry", () => {
  test("a screen asks for an entry by its identifier and lands on Connections", async () => {
    // GIVEN no entry asked for
    expect(get(pendingConnectionAdd)).toBeNull();

    // WHEN the preview of a package asks for the entry of Notion
    requestConnectionAdd("com.notion/mcp");
    await vi.waitFor(() => expect(get(currentRoute)).toBe("integrations"));

    // THEN Connections takes the entry once, and only once
    expect(takePendingConnectionAdd()).toBe("com.notion/mcp");
    expect(takePendingConnectionAdd()).toBeNull();
  });

  test("the entry is found by its registry name or by its package", () => {
    // GIVEN a selection of a remote entry and a package entry
    const selection = [entry("com.notion/mcp"), entry("io.linear/mcp", "@linear/mcp-server")];

    // WHEN entries are looked up by each identifier
    // THEN each identifier finds its entry, an unknown one none
    expect(findSelectionEntry(selection, "com.notion/mcp")?.name).toBe("com.notion/mcp");
    expect(findSelectionEntry(selection, "@linear/mcp-server")?.name).toBe("io.linear/mcp");
    expect(findSelectionEntry(selection, "github")).toBeNull();
  });
});
