/**
 * Settings > Integrations: its place in the column, the accesses a new agent
 * is granted, the profile default, the request MCP row of the developer mode,
 * the summary of what is always authorized and its two links.
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { get } from "svelte/store";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ResolvedSetting } from "$lib/ipc/settings";
import { SETTINGS_NAV, visibleSettingsNav } from "$lib/settings/settingsNav";
import { currentRoute } from "$lib/stores/navigation";
import { SETTINGS_SUB_ROUTES } from "$lib/stores/settings";
import { settingsWindowOpen } from "$lib/stores/settingsWindow";
import { hydrateShareProfileDefault, shareProfileDefault } from "$lib/stores/shareProfileDefault";
import { settingsToggleState } from "../../components/settings/SettingsToggle.svelte";
import {
  ALLOW_REQUEST_MCP_KEY,
  INTEGRATIONS_BLOCKS,
  INTEGRATIONS_TEST_IDS,
  INTEGRATION_DEFAULTS,
  NEW_AGENT_ACCESS_KEY,
  NEW_AGENT_FAMILIES_KEY,
  SHARE_PROFILE_DEFAULT_KEY,
  alwaysSummary,
  defaultAccessRows,
  openAuthorizations,
  openConnections,
  readIntegrationDefaults,
  rowLocked,
  toggledAccess,
  visibleIntegrationsBlocks,
  writeAllowRequestMcp,
  writeDefaultAccess,
  writeShareProfile,
  type SettingsIpc,
} from "./integrations";

const SRC = resolve(__dirname, "../..");
const source = (path: string): string => readFileSync(resolve(SRC, path), "utf8");
const catalogue = (locale: "fr" | "en"): Record<string, unknown> =>
  JSON.parse(source(`lib/i18n/${locale}.json`)) as Record<string, unknown>;

function lookup(tree: Record<string, unknown>, key: string): unknown {
  return key
    .split(".")
    .reduce<unknown>(
      (node, part) =>
        typeof node === "object" && node !== null
          ? (node as Record<string, unknown>)[part]
          : undefined,
      tree,
    );
}

/** What the shell answers for a key nobody wrote: the defaults of `apollia.toml`. */
const FILE_DEFAULTS: Record<string, unknown> = {
  [NEW_AGENT_FAMILIES_KEY]: ["read"],
  [NEW_AGENT_ACCESS_KEY]: [],
  [SHARE_PROFILE_DEFAULT_KEY]: true,
  [ALLOW_REQUEST_MCP_KEY]: false,
};

/**
 * A shell that holds its settings in memory, as the real one resolves them:
 * the value the policy imposes (`imposed`), else the one written, else the
 * default. A write to an imposed key is refused.
 */
function shell(
  written: Record<string, unknown> = {},
  imposed: Record<string, unknown> = {},
): SettingsIpc & { written: [string, unknown][] } {
  const writes: [string, unknown][] = [];
  const resolved = (key: string): ResolvedSetting => {
    if (key in imposed) return { key, value: imposed[key], source: "policy" };
    if (key in written) return { key, value: written[key], source: "user" };
    return { key, value: FILE_DEFAULTS[key], source: "default" };
  };
  return {
    written: writes,
    read: async (key) => resolved(key),
    write: async (key, value) => {
      if (key in imposed) {
        throw new Error(`setting '${key}' is locked by the organization policy`);
      }
      writes.push([key, value]);
      written[key] = value;
      return resolved(key);
    },
  };
}

const TOOLS = [
  { name: "file_write", family: "write_or_delete" },
  { name: "web_search", family: "read" },
  { name: "ask_user", family: "exempt" },
  { name: "bash_executor", family: "execute_code" },
  { name: "code_context", family: "read" },
  { name: "file_read", family: "read" },
  { name: "memory_search", family: "read_personal" },
];

describe("Integrations, a section of the settings", () => {
  it("takes its place in the Settings group, between the model defaults and Notifications", () => {
    // GIVEN the Settings group of the column
    const settings = SETTINGS_NAV[0].entries.map((entry) => entry.key);

    // WHEN the place of Integrations is read
    const at = settings.indexOf("integrations");

    // THEN it follows the model defaults, precedes Notifications, is a
    // sub-route of the store, and shows whatever the developer mode
    expect(settings[at - 1]).toBe("model-defaults");
    expect(settings[at + 1]).toBe("notifications");
    expect(SETTINGS_SUB_ROUTES).toContain("integrations");
    for (const mode of [false, true]) {
      expect(visibleSettingsNav(mode)[0].entries.map((entry) => entry.key)).toContain("integrations");
    }
  });

  it("mounts its own section, which holds nothing of the former OAuth section", () => {
    // GIVEN the settings shell and the section it loads
    const shellSource = source("routes/Settings.svelte");
    const section = source("routes/settings/Integrations.svelte");

    // WHEN the loader and the section are read
    // THEN the shell loads the section, and the section names neither a
    // credential field nor an OAuth catalogue key
    expect(shellSource).toContain('integrations: () => import("./settings/Integrations.svelte")');
    expect(section).not.toMatch(/Oauth|client_id|client_secret|settings\.integrations\./i);
  });

  it.each(["fr", "en"] as const)("names every text of the section in the %s catalogue", (locale) => {
    // GIVEN one catalogue and the keys the section reads
    const tree = catalogue(locale);
    const section = source("routes/settings/Integrations.svelte");
    const keys = new Set([
      "settings.nav.integrations",
      ...INTEGRATIONS_BLOCKS,
      ...[...section.matchAll(/"((?:settings\.integrations_section|dev\.integrations)\.[a-z_]+)"/g)].map(
        (m) => m[1],
      ),
    ]);

    // WHEN each is resolved
    // THEN each is a non-empty string, and the section reads more than its titles
    expect(keys.size).toBeGreaterThan(INTEGRATIONS_BLOCKS.length + 10);
    for (const key of keys) {
      const text = lookup(tree, key);
      expect(typeof text, `${key} in ${locale}`).toBe("string");
      expect((text as string).length, `${key} in ${locale}`).toBeGreaterThan(0);
    }
  });

  it("says share, not inject, for the profile of a new conversation", () => {
    // GIVEN the two catalogues
    // WHEN the label of the profile default is read
    const label = (locale: "fr" | "en"): string =>
      lookup(catalogue(locale), "settings.integrations_section.share_profile_label") as string;

    // THEN it is the wording validated with the mock-up: neither says inject
    expect(label("en")).toBe("Share my profile with a new conversation");
    expect(label("fr")).not.toMatch(/inject/i);
    expect(label("fr")).toMatch(/profil/);
  });
});

describe("the accesses a new agent created without code is granted", () => {
  it("checks the reading family, leaves the sensitive accesses unchecked and marked", () => {
    // GIVEN the tools of a machine and the default of a fresh installation
    // WHEN the lines are built
    const rows = defaultAccessRows(TOOLS, ["read"], []);

    // THEN the reading family comes first, checked and held by its family; the
    // sensitive accesses follow, unchecked and marked; the tool that acts on
    // the conversation alone and the code context are not accesses to give
    expect(rows).toEqual([
      { id: "file_read", checked: true, sensitive: false, byFamily: true },
      { id: "web_search", checked: true, sensitive: false, byFamily: true },
      { id: "bash_executor", checked: false, sensitive: true, byFamily: false },
      { id: "file_write", checked: false, sensitive: true, byFamily: false },
      { id: "memory_search", checked: false, sensitive: true, byFamily: false },
    ]);
  });

  it("widens the default with an access checked here, and narrows it back", async () => {
    // GIVEN the default of a fresh installation
    const ipc = shell();
    const fresh = await readIntegrationDefaults(ipc);

    // WHEN a sensitive access is checked, then unchecked
    const widened = await writeDefaultAccess(fresh, "file_write", true, ipc);
    const rows = defaultAccessRows(TOOLS, widened.families, widened.access);
    const narrowed = await writeDefaultAccess(widened, "file_write", false, ipc);

    // THEN the named accesses are written as a list, the line reads checked
    // and still marked sensitive, and unchecking writes the list without it
    expect(fresh.families).toEqual(["read"]);
    expect(ipc.written).toEqual([
      [NEW_AGENT_ACCESS_KEY, ["file_write"]],
      [NEW_AGENT_ACCESS_KEY, []],
    ]);
    expect(rows.find((row) => row.id === "file_write")).toEqual({
      id: "file_write",
      checked: true,
      sensitive: true,
      byFamily: false,
    });
    expect(narrowed.access).toEqual([]);
    expect(toggledAccess(["a", "b"], "a", true)).toEqual(["b", "a"]);
  });

  it("locks every line when the policy imposes the default, and refuses the write", async () => {
    // GIVEN a policy that imposes the named accesses
    const ipc = shell({}, { [NEW_AGENT_ACCESS_KEY]: ["file_write"] });
    const imposed = await readIntegrationDefaults(ipc);
    const rows = defaultAccessRows(TOOLS, imposed.families, imposed.access);

    // WHEN the lines are read and a write is attempted
    const attempt = writeDefaultAccess(imposed, "bash_executor", true, ipc);

    // THEN the source is the policy, no line can be checked or unchecked, and
    // the refusal reaches the caller with nothing written
    expect(imposed.accessSource).toBe("policy");
    expect(rows.every((row) => rowLocked(row, imposed.accessSource))).toBe(true);
    await expect(attempt).rejects.toThrow("locked");
    expect(ipc.written).toEqual([]);
  });

  it("holds a line of the reading family even without a policy", () => {
    // GIVEN the lines of a fresh installation
    const rows = defaultAccessRows(TOOLS, ["read"], []);

    // WHEN each is asked whether it can be changed here
    const locked = rows.filter((row) => rowLocked(row, "default")).map((row) => row.id);

    // THEN the family lines are held, the sensitive ones are free
    expect(locked).toEqual(["file_read", "web_search"]);
  });

  it("shows the default of a fresh installation when the shell does not answer", async () => {
    // GIVEN a shell that cannot be reached
    const down: SettingsIpc = {
      read: async () => {
        throw new Error("shell unreachable");
      },
      write: async () => {
        throw new Error("shell unreachable");
      },
    };

    // WHEN the defaults are read
    const read = await readIntegrationDefaults(down);

    // THEN nothing wider than a fresh installation is shown
    expect(read).toEqual(INTEGRATION_DEFAULTS);
    expect(read.families).toEqual(["read"]);
    expect(read.access).toEqual([]);
    expect(read.allowRequestMcp).toBe(false);
  });
});

describe("the profile in a new conversation", () => {
  beforeEach(() => {
    shareProfileDefault.set(true);
  });

  it("is on by default, and the default turned off here reaches the store the chat reads", async () => {
    // GIVEN a fresh installation
    const ipc = shell();
    const fresh = await readIntegrationDefaults(ipc);
    await hydrateShareProfileDefault(ipc.read);
    const before = get(shareProfileDefault);

    // WHEN the default is turned off and the store reads it again
    const off = await writeShareProfile(fresh, false, ipc);
    await hydrateShareProfileDefault(ipc.read);

    // THEN the key of the chat section is written, and the store follows
    expect(fresh.shareProfile).toBe(true);
    expect(before).toBe(true);
    expect(ipc.written).toEqual([[SHARE_PROFILE_DEFAULT_KEY, false]]);
    expect(SHARE_PROFILE_DEFAULT_KEY).toBe("chat.share_profile_default");
    expect(off.shareProfile).toBe(false);
    expect(get(shareProfileDefault)).toBe(false);
  });

  it("keeps the profile shared when the shell does not answer", async () => {
    // GIVEN a shell that cannot be reached
    // WHEN the store is seeded
    await hydrateShareProfileDefault(async () => {
      throw new Error("shell unreachable");
    });

    // THEN the default of a fresh installation stays
    expect(get(shareProfileDefault)).toBe(true);
  });

  it("is shown locked when the policy imposes it", async () => {
    // GIVEN a policy that imposes the sharing off
    const ipc = shell({}, { [SHARE_PROFILE_DEFAULT_KEY]: false });

    // WHEN the section reads the default
    const imposed = await readIntegrationDefaults(ipc);

    // THEN the row is disabled and names the policy
    expect(imposed.shareProfile).toBe(false);
    expect(settingsToggleState(imposed.shareProfileSource, false)).toEqual({
      disabled: true,
      policyLabelKey: "settings.policy_locked",
    });
  });
});

describe("the MCP servers a request may bring", () => {
  it("is a row of the developer mode only", () => {
    // GIVEN the blocks of the section and its markup
    const section = source("routes/settings/Integrations.svelte");

    // WHEN the blocks are listed with the mode off, then on
    // THEN the developer block shows only with the mode on, and the row is
    // rendered under the mode alone
    expect(visibleIntegrationsBlocks(false)).toEqual(INTEGRATIONS_BLOCKS.slice(0, 4));
    expect(visibleIntegrationsBlocks(true)).toEqual(INTEGRATIONS_BLOCKS);
    const guard = section.indexOf("{#if $developerMode}");
    const row = section.indexOf("INTEGRATIONS_TEST_IDS.requestMcp");
    expect(guard).toBeGreaterThan(-1);
    expect(row).toBeGreaterThan(guard);
    expect(section.indexOf("{/if}", row)).toBeGreaterThan(row);
  });

  it("is off by default and writes the key the local server reads", async () => {
    // GIVEN a fresh installation
    const ipc = shell();
    const fresh = await readIntegrationDefaults(ipc);

    // WHEN the row is turned on
    const on = await writeAllowRequestMcp(fresh, true, ipc);

    // THEN the key of the compatible server is written, and reads on
    expect(fresh.allowRequestMcp).toBe(false);
    expect(ALLOW_REQUEST_MCP_KEY).toBe("server.allow_request_mcp");
    expect(ipc.written).toEqual([["server.allow_request_mcp", true]]);
    expect(on.allowRequestMcp).toBe(true);
    expect(on.allowRequestMcpSource).toBe("user");
  });

  it("reads the keys of the default access by their dotted names", async () => {
    // GIVEN a shell that records what it is asked
    const asked: string[] = [];
    const ipc: SettingsIpc = {
      read: async (key) => {
        asked.push(key);
        return { key, value: undefined, source: "default" };
      },
      write: vi.fn(),
    };

    // WHEN the defaults are read
    await readIntegrationDefaults(ipc);

    // THEN the four settings of the section are asked, and no other
    expect(asked.sort()).toEqual(
      [
        ALLOW_REQUEST_MCP_KEY,
        NEW_AGENT_ACCESS_KEY,
        NEW_AGENT_FAMILIES_KEY,
        SHARE_PROFILE_DEFAULT_KEY,
      ].sort(),
    );
  });
});

describe("what is always authorized", () => {
  it("counts the standing consents of each agent, by name", () => {
    // GIVEN standing consents of two agents, one of them unnamed
    const consents = [
      { agent_id: "invoices" },
      { agent_id: "apollia:chat" },
      { agent_id: "invoices" },
      { agent_id: "invoices" },
    ];
    const names: Record<string, string> = { "apollia:chat": "Apollia" };

    // WHEN the summary is built
    const lines = alwaysSummary(consents, (id) => names[id]);

    // THEN each agent has one line with its count, an unnamed agent shown by
    // its identifier, and no consent means no line
    expect(lines).toEqual([
      { agentId: "apollia:chat", name: "Apollia", count: 1 },
      { agentId: "invoices", name: "invoices", count: 3 },
    ]);
    expect(alwaysSummary([], () => undefined)).toEqual([]);
  });

  it("is read only: the section withdraws nothing", () => {
    // GIVEN the section
    const section = source("routes/settings/Integrations.svelte");

    // WHEN its imports are read
    // THEN it lists the consents and reaches no command that changes them
    expect(section).toContain("listFamilyConsents");
    expect(section).not.toMatch(/revokeFamilyConsent|restoreFamilyConsent|governanceRevoke/);
  });
});

describe("the two links of the section", () => {
  beforeEach(() => {
    currentRoute.set("chat");
    settingsWindowOpen.set(true);
  });

  it("opens Authorizations and closes the settings window", () => {
    // GIVEN the settings window open above the Chat
    // WHEN the link of the summary is followed
    // THEN the Authorizations screen shows, the window closed, without error
    expect(() => openAuthorizations()).not.toThrow();
    expect(get(currentRoute)).toBe("authorizations");
    expect(get(settingsWindowOpen)).toBe(false);
  });

  it("opens Connections and closes the settings window", () => {
    // GIVEN the settings window open above the Chat
    // WHEN the link to the connections is followed
    // THEN the Connections screen shows, the window closed, without error
    expect(() => openConnections()).not.toThrow();
    expect(get(currentRoute)).toBe("integrations");
    expect(get(settingsWindowOpen)).toBe(false);
  });

  it("gives each link and each default a test identifier of its own", () => {
    // GIVEN the identifiers of the section and its markup
    const section = source("routes/settings/Integrations.svelte");
    const ids = Object.values(INTEGRATIONS_TEST_IDS);

    // WHEN they are compared
    // THEN they are distinct, and the two links and the two switches are wired
    expect(new Set(ids).size).toBe(ids.length);
    for (const name of ["openAuthorizations", "openConnections", "shareProfile", "requestMcp"]) {
      expect(section).toContain(`INTEGRATIONS_TEST_IDS.${name}`);
    }
  });
});
