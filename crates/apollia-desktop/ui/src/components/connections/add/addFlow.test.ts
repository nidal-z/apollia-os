import { describe, test, expect, vi } from "vitest";
import { existsSync, readFileSync } from "node:fs";
import { render as renderToString } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null) }));

// Stores read the browser storage at import; node has none.
vi.hoisted(() => {
  const map = new Map<string, string>();
  globalThis.localStorage = {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
    removeItem: (k: string) => void map.delete(k),
    clear: () => map.clear(),
    key: () => null,
    length: 0,
  } as Storage;
});

import "$lib/i18n";
import fr from "$lib/i18n/fr.json";
import en from "$lib/i18n/en.json";
import { developerMode } from "$lib/stores/developerMode";
import { APOLLIA_AGENT_ID } from "$lib/chat/chatTargets";
import AddFlow from "./AddFlow.svelte";
import AddressForm from "./AddressForm.svelte";
import WhoCanUse from "./WhoCanUse.svelte";
import TrialSucceeded from "./TrialSucceeded.svelte";
import WizardStepAuth from "../../integrations/WizardStepAuth.svelte";
import ConnectionRefusal from "../../integrations/ConnectionRefusal.svelte";
import { REMOTE_DENIED_REASON } from "$lib/integrations/addConnection";
import {
  KEY_HEADER,
  EMPTY_ADDRESS_FORM,
  addressConfig,
  connectGesture,
  fieldLabel,
  grantToAgents,
  keptOnceAdded,
  plainScopes,
  stepOf,
  whoChoices,
} from "$lib/connections/addFlow";
import { installedNameFor, isInstalled } from "$lib/connections/vendor";
import { deriveServerName } from "$lib/integrations/connectorConfig";
import type { AddEntry } from "$lib/connections/addWindow";
import type { AgentListItem, Connection, RegistryServerView } from "$lib/types";

const noop = () => {};

function source(rel: string): string {
  return readFileSync(new URL(rel, import.meta.url), "utf8");
}

/** The text a user reads: tags, attributes and comments removed. */
function visible(html: string): string {
  return html
    .replace(/<!--[\s\S]*?-->/g, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/\s+/g, " ");
}

async function inLocale(lang: "fr" | "en", dev = false): Promise<void> {
  locale.set(lang);
  await waitLocale(lang);
  developerMode.set(dev);
}

const NOTION: RegistryServerView = {
  name: "com.notion/mcp",
  title: "Notion",
  description: "Pages and databases",
  version: "1.0.0",
  repository_url: null,
  website_url: null,
  packages: null,
  trust_level: "verified_official",
  category: "productivity",
  enrichment: null,
  is_installed: false,
  remotes: [{ type: "streamable-http", url: "https://mcp.notion.com/mcp", headers: [] }],
} as unknown as RegistryServerView;

const ENTRIES: Record<string, AddEntry> = {
  catalogue: { key: "entry:com.notion/mcp", kind: "catalogue", entry: NOTION, source: "selection" },
  address: { key: "address", kind: "address" },
  mailbox: { key: "mailbox", kind: "mailbox" },
  folder: { key: "folder", kind: "folder" },
  google: { key: "native:google", kind: "native", provider: "google" },
  microsoft: { key: "native:microsoft", kind: "native", provider: "microsoft" },
};

function flow(entry: AddEntry, extra: Record<string, unknown> = {}): string {
  return renderToString(AddFlow as never, {
    props: { entry, name: "Notion", onback: noop, ondone: noop, onopenrule: noop, ...extra } as never,
  }).body;
}

function agent(name: string, type: AgentListItem["agent_type"] = "worker"): AgentListItem {
  return { name, agent_type: type } as unknown as AgentListItem;
}

function connection(id: string, tools: Array<[string, string]>): Connection {
  return {
    id,
    kind: "mcp",
    label: id,
    account: null,
    provenance: "custom",
    remote: false,
    official: false,
    state: "connected",
    status: "working",
    reason: null,
    checkedAt: null,
    package: null,
    tools: tools.map(([toolId, family]) => ({ id: toolId, family, familySource: "server", prechecked: false })),
  } as unknown as Connection;
}

describe("one flow serves every kind of connection", () => {
  test.each(Object.keys(ENTRIES))("the %s entry opens the same three steps", async (kind) => {
    // GIVEN the interface in French
    await inLocale("fr");

    // WHEN the flow of the entry is rendered
    const html = flow(ENTRIES[kind]);

    // THEN it is the one flow, on its first step, the three steps named in order
    expect(html).toContain('data-testid="connector-wizard"');
    expect(html).toContain('data-phase="connect"');
    expect(html).toContain('data-step="connect"');
    const text = visible(html);
    const steps = [fr.connections.add_flow.steps.connect, fr.connections.add_flow.steps.who, fr.connections.add_flow.steps.trial];
    const at = steps.map((step) => text.indexOf(step));
    expect(at.every((i) => i >= 0)).toBe(true);
    expect([...at].sort((a, b) => a - b)).toEqual(at);
  });

  test("each kind asks its own gesture to connect, inside that flow", async () => {
    // GIVEN the interface in French
    await inLocale("fr");

    // WHEN the first step of each kind is rendered
    const html = Object.fromEntries(Object.entries(ENTRIES).map(([k, e]) => [k, flow(e)]));

    // THEN the warning of an entry, the address form, the mailbox form, the
    // folder picker and the sign-in each stand in the same flow
    expect(html.catalogue).toContain('data-testid="wizard-step-disclaimer"');
    expect(html.catalogue).toContain('data-testid="wizard-warning-accept"');
    expect(html.catalogue).not.toContain('type="checkbox"');
    expect(html.address).toContain('data-testid="add-flow-address"');
    expect(html.mailbox).toContain('data-testid="mailbox-email"');
    expect(html.folder).toContain('data-testid="add-flow-folder-pick"');
    expect(html.google).toContain('data-testid="add-flow-signin"');
    expect(html.microsoft).toContain('data-testid="add-flow-signin"');
    expect(visible(html.microsoft)).toContain(fr.connections.add_flow.sign_in_work_account);
    expect(visible(html.google)).not.toContain(fr.connections.add_flow.sign_in_work_account);
    expect(Object.values(ENTRIES).map(connectGesture)).toEqual([
      "warning",
      "address",
      "mailbox",
      "folder",
      "sign_in",
      "sign_in",
    ]);
  });

  test("the five former dialogs are gone, and every add path goes through the flow", () => {
    // GIVEN the tree of the interface
    const gone = [
      "../../integrations/ConnectorWizard.svelte",
      "../catalogue/CustomMcpForm.svelte",
      "../../integrations/MailboxDialog.svelte",
      "../catalogue/FolderConnectionDialog.svelte",
      "../../integrations/GrantApolliaOption.svelte",
    ];

    // WHEN the files and the two places that add a connection are read
    const window = source("../catalogue/AddConnectionWindow.svelte");
    const onboarding = source("../../onboarding/OnboardingConnection.svelte");

    // THEN none of the five remains, and both places mount the one flow
    for (const rel of gone) expect(existsSync(new URL(rel, import.meta.url)), rel).toBe(false);
    expect(window).toContain("<AddFlow");
    expect(onboarding).toContain("<AddFlow");
  });

  test("the steps follow the phases: connect, who may use it, the trial", () => {
    // GIVEN every phase of the flow
    // WHEN the step of each is read
    // THEN working, a refusal and a failure belong to the first step
    expect(stepOf("connect")).toBe("connect");
    expect(stepOf("working")).toBe("connect");
    expect(stepOf("refused")).toBe("connect");
    expect(stepOf("failed")).toBe("connect");
    expect(stepOf("who")).toBe("who");
    expect(stepOf("trial")).toBe("trial");
  });

  test("the component Node.js is is named only in developer mode", () => {
    // GIVEN the flow and the two catalogues
    const text = source("./AddFlow.svelte");

    // WHEN the download message is read in each mode
    // THEN the default text says a required component, the developer text names Node.js
    expect(fr.integrations.wizard.component_downloading).not.toContain("Node");
    expect(fr.dev.integrations.wizard.node_downloading).toContain("Node.js");
    expect(text).toContain('? $t("dev.integrations.wizard.node_downloading")');
  });
});

describe("who may use it", () => {
  test("Apollia comes first and checked, the other agents to check, the system ones left out", async () => {
    // GIVEN three agents of this machine, one of them a system agent
    const agents = [agent("writer"), agent("apollia-guide", "system"), agent("analyst")];

    // WHEN the step lists them
    const choices = whoChoices(agents, "Apollia");
    await inLocale("fr");
    const html = renderToString(WhoCanUse as never, {
      props: { agents, loading: false, checked: [APOLLIA_AGENT_ID] } as never,
    }).body;

    // THEN Apollia leads, then the others by name, and only Apollia is checked
    expect(choices.map((c) => c.id)).toEqual([APOLLIA_AGENT_ID, "analyst", "writer"]);
    const rows = html.split('data-testid="add-flow-who-agent"').slice(1);
    expect(rows).toHaveLength(3);
    expect(rows[0]).toContain(`data-agent="${APOLLIA_AGENT_ID}"`);
    expect(rows[0]).toContain('aria-checked="true"');
    expect(rows[1]).toContain('aria-checked="false"');
    expect(rows[2]).toContain('aria-checked="false"');
    expect(visible(html)).toContain(fr.connections.add_flow.who_rule);
  });

  test("a sign-in done in its own dialog resumes the flow on this step", async () => {
    // GIVEN an account just connected in the dialog of its provider
    await inLocale("fr");

    // WHEN the flow resumes on it
    const html = flow(ENTRIES.microsoft, { resume: "native:microsoft:ada@example.com" });

    // THEN it stands on who may use it, Apollia checked
    expect(html).toContain('data-step="who"');
    expect(html).toContain('data-testid="add-flow-who"');
    expect(html).toContain('data-testid="add-flow-who-continue"');
  });

  test("the agents checked get every tool of the connection, sensitive and unclassified alike", async () => {
    // GIVEN a server whose tools appear on the second read: one that reads, one
    // that writes, one nobody classified
    const tools: Array<[string, string]> = [
      ["mcp:crm/search", "read"],
      ["mcp:crm/delete", "write_or_delete"],
      ["mcp:crm/sync", "unclassified"],
    ];
    const list = vi
      .fn()
      .mockResolvedValueOnce([connection("mcp:crm", [])])
      .mockResolvedValue([connection("mcp:crm", tools)]);
    const grant = vi.fn().mockResolvedValue({});

    // WHEN Apollia and one agent are granted it
    const granted = await grantToAgents("mcp:crm", [APOLLIA_AGENT_ID, "analyst"], { list, grant, waitMs: 0 });

    // THEN one grant names both agents and every tool, the approval left to each call
    expect(granted).toBe(3);
    expect(grant).toHaveBeenCalledTimes(1);
    expect(grant).toHaveBeenCalledWith("mcp:crm", [APOLLIA_AGENT_ID, "analyst"], tools.map(([id]) => id));
    expect(list).toHaveBeenCalledTimes(2);
  });

  test("a folder is granted whole, as any other connection", async () => {
    // GIVEN a folder that reads and writes
    const folder = { ...connection("folder:f1", [["file_read", "read"], ["file_edit", "write_or_delete"]]), kind: "folder" } as Connection;
    const grant = vi.fn().mockResolvedValue({});

    // WHEN Apollia is granted it
    await grantToAgents("folder:f1", [APOLLIA_AGENT_ID], { list: async () => [folder], grant, waitMs: 0 });

    // THEN writing comes with reading, its approval asked at each call
    expect(grant).toHaveBeenCalledWith("folder:f1", [APOLLIA_AGENT_ID], ["file_read", "file_edit"]);
  });

  test("no agent checked writes nothing", async () => {
    // GIVEN every box unchecked
    const list = vi.fn();
    const grant = vi.fn();

    // WHEN the step goes on
    const granted = await grantToAgents("mcp:crm", [], { list, grant });

    // THEN nothing is read nor granted
    expect(granted).toBe(0);
    expect(list).not.toHaveBeenCalled();
    expect(grant).not.toHaveBeenCalled();
  });

  test("the grant goes through the command of the connection, never through each agent", () => {
    // GIVEN the flow and its logic
    const logic = source("../../../lib/connections/addFlow.ts");
    const component = source("./AddFlow.svelte");

    // WHEN the grant is read
    // THEN the flow calls the wrapper of the grant command, once for every agent checked
    expect(logic).toContain('import { grantConnection, listConnections } from "$lib/ipc/connections";');
    expect(logic).toContain("await grant(connectionId, [...agents], tools);");
    expect(component).toContain("await grantToAgents(id, checked);");
    expect(logic).not.toContain("getAgentDefinition");
  });
});

describe("a service by its address", () => {
  test("an address and a key: the key travels as its authorization, outside the settings", () => {
    // GIVEN an address and the key the service gave
    const form = { ...EMPTY_ADDRESS_FORM, url: "https://tools.example.com/connect", key: "k-123" };

    // WHEN its configuration is built, out of developer mode
    const built = addressConfig(form, false);

    // THEN it is reached at the address, named after it, the key in its one header
    expect(built).toEqual({
      kind: "ok",
      config: {
        name: "tools-example-com",
        url: "https://tools.example.com/connect",
        env: { [KEY_HEADER]: "Bearer k-123" },
        transport: "streamable-http",
        tags: [],
      },
    });
  });

  test("a local command, headers and variables are the developer mode's", () => {
    // GIVEN a form that holds a command and headers, as the developer mode fills it
    const form = {
      ...EMPTY_ADDRESS_FORM,
      url: "https://tools.example.com/connect",
      transport: "stdio" as const,
      command: "uvx",
      args: "acme-server --stdio",
      headers: "ACME_TOKEN=abc\n# comment\nDEBUG=1",
    };

    // WHEN it is built in each mode
    const operator = addressConfig(form, false);
    const developer = addressConfig(form, true);

    // THEN out of developer mode only the address counts; in it, the command and its variables
    expect(operator.kind === "ok" && operator.config).toMatchObject({
      url: "https://tools.example.com/connect",
      env: {},
      transport: "streamable-http",
    });
    expect(operator.kind === "ok" && operator.config.command).toBeUndefined();
    expect(developer).toEqual({
      kind: "ok",
      config: {
        name: "uvx",
        command: "uvx",
        args: ["acme-server", "--stdio"],
        env: { ACME_TOKEN: "abc", DEBUG: "1" },
        transport: "stdio",
        tags: [],
      },
    });
    expect(addressConfig(EMPTY_ADDRESS_FORM, false)).toEqual({ kind: "missing", field: "url" });
  });

  test("the form shows the address, offers a key, and keeps the technical fields for the developer mode", async () => {
    // GIVEN the form, in French then in English, the developer mode off
    for (const lang of ["fr", "en"] as const) {
      await inLocale(lang);

      // WHEN it is rendered
      const html = renderToString(AddressForm as never, { props: { oncancel: noop, onsubmit: noop } as never }).body;
      const text = visible(html);

      // THEN the address and the offer of a key are there, nothing technical
      expect(html).toContain('data-testid="custom-mcp-url-input"');
      expect(html).toContain('data-testid="add-flow-key-toggle"');
      for (const id of ["custom-mcp-transport-select", "custom-mcp-command-input", "custom-mcp-env-input"]) {
        expect(html).not.toContain(id);
      }
      for (const word of ["Bearer", "Authorization", "HTTP", "Headers", "En-têtes", "NAME=value", "stdio"]) {
        expect(text).not.toContain(word);
      }
    }

    // AND in developer mode the transport, the headers and their syntax are offered
    await inLocale("en", true);
    const dev = renderToString(AddressForm as never, { props: { oncancel: noop, onsubmit: noop } as never }).body;
    expect(dev).toContain('data-testid="custom-mcp-transport-select"');
    expect(dev).toContain('data-testid="custom-mcp-env-input"');
    expect(visible(dev)).toContain(en.dev.connections.custom_headers_label);
    developerMode.set(false);
  });
});

describe("no technical label out of developer mode", () => {
  const KEY = { name: "BRAVE_API_KEY", description: "Your Brave Search key", isRequired: true, isSecret: true };
  const VALUE = { name: "ACME_REGION", description: "The region of your account", isRequired: true, isSecret: false };
  const DISCOVERY = {
    as_url: "https://auth.example.com",
    scopes_supported: ["read:pages", "write:pages"],
    scope_descriptions: { "read:pages": "Read your pages" },
    registration_supported: true,
  };

  function auth(): string {
    return renderToString(WizardStepAuth as never, {
      props: { envVars: [KEY, VALUE], enrichment: null, values: {}, onchange: noop, probeMode: "static" } as never,
    }).body;
  }

  function oauth(): string {
    return renderToString(WizardStepAuth as never, {
      props: {
        envVars: [],
        enrichment: null,
        values: {},
        onchange: noop,
        probeMode: "oauth",
        oauthDiscovery: DISCOVERY,
        oauthSelectedScopes: ["read:pages", "write:pages"],
      } as never,
    }).body;
  }

  test("a field reads as a key or by what it is, its name kept for the developer mode", async () => {
    // GIVEN an entry asking a key and a value, the interface in French
    await inLocale("fr");

    // WHEN its fields are rendered in each mode
    const operator = visible(auth());
    developerMode.set(true);
    const developer = visible(auth());
    developerMode.set(false);

    // THEN the operator reads "Clé d'accès" and the description, never the variable names
    expect(operator).toContain(fr.connections.add_flow.key_label);
    expect(operator).toContain("The region of your account");
    expect(operator).not.toContain("BRAVE_API_KEY");
    expect(operator).not.toContain("ACME_REGION");
    expect(developer).toContain("BRAVE_API_KEY");
    expect(fieldLabel(KEY, false, { key: "Key", value: "Value" })).toBe("Key");
    expect(fieldLabel({ ...VALUE, description: " " }, false, { key: "Key", value: "Value" })).toBe("Value");
  });

  test("a sign-in names what it grants, its scope identifiers kept for the developer mode", async () => {
    // GIVEN a service whose sign-in asks two scopes, one described
    await inLocale("fr");

    // WHEN the sign-in step is rendered in each mode
    const operator = oauth();
    developerMode.set(true);
    const developer = oauth();
    developerMode.set(false);

    // THEN the operator reads the description only, the developer the identifiers to check
    expect(visible(operator)).toContain("Read your pages");
    expect(visible(operator)).not.toContain("read:pages");
    expect(operator).not.toContain('type="checkbox"');
    expect(developer).toContain('data-testid="auth-oauth-scope-read:pages"');
    expect(plainScopes(DISCOVERY.scope_descriptions, DISCOVERY.scopes_supported)).toEqual(["Read your pages"]);
  });
});

describe("a server of 0.3.0 named after its label", () => {
  // A remote entry: no package to match it by.
  const BROWSER = {
    ...NOTION,
    name: "io.example/web-browser",
    title: "Browser",
    enrichment: { operator_label: { en: "Web Browser", fr: "Navigateur web" } },
    remotes: [{ type: "streamable-http", url: "https://browser.example.com/mcp", headers: [] }],
  } as unknown as RegistryServerView;

  test("is seen installed under its former name, and opened rather than added again", () => {
    // GIVEN a server installed in 0.3.0 under the English label of its entry
    const installed = new Set(["web-browser"]);

    // WHEN the entry is looked up among the servers of this machine
    const name = installedNameFor(BROWSER, installed, new Map());

    // THEN it is found under that name, the entry reads installed, and its
    // card opens it instead of installing a second server
    expect(deriveServerName(BROWSER)).toBe("io-example-web-browser");
    expect(name).toBe("web-browser");
    expect(isInstalled(BROWSER, installed, new Map())).toBe(true);
  });

  test("is seen installed under the name drawn from its identifier", () => {
    // GIVEN the same entry installed since, under its identifier
    const installed = new Set(["io-example-web-browser"]);

    // WHEN it is looked up
    // THEN it is found, and an unrelated server is not taken for it
    expect(installedNameFor(BROWSER, installed, new Map())).toBe("io-example-web-browser");
    expect(installedNameFor(BROWSER, new Set(["browser-tools"]), new Map())).toBeNull();
  });
});

describe("the refusal box", () => {
  test("says the organisation refused, why, and leads to its policy", async () => {
    // GIVEN a refusal of an online connection by the organisation policy
    await inLocale("fr");
    const refusal = { code: "remote_denied" as const, by: "organisation" as const, reason: REMOTE_DENIED_REASON };

    // WHEN the box is rendered
    const { body } = renderToString(ConnectionRefusal, { props: { refusal, onopen: noop } });

    // THEN it names who refused, the reason in plain words, and its action
    expect(body).toContain(fr.connections.refusal.organisation_title);
    expect(body).toContain(fr.connections.refusal.remote_denied);
    expect(body).toContain(fr.connections.refusal.open_policy);
    expect(body).not.toContain(REMOTE_DENIED_REASON);
  });

  test("a refusal by the person's permissions leads to the permissions", async () => {
    // GIVEN a refusal by the permissions
    await inLocale("fr");
    const refusal = { code: "remote_denied" as const, by: "permissions" as const, reason: "" };

    // WHEN the box is rendered
    const { body } = renderToString(ConnectionRefusal, { props: { refusal, onopen: noop } });

    // THEN it says so and leads to the permissions, with no administrator to ask
    expect(body).toContain(fr.connections.refusal.permissions_title);
    expect(body).toContain(fr.connections.refusal.open_permissions);
    expect(body).not.toContain(fr.connections.refusal.ask_administrator);
  });
});

describe("what the former dialogs said and offered, kept in the flow", () => {
  const FOLDER = {
    ...connection("folder:clients", [
      ["file_read", "read"],
      ["file_write", "write_or_delete"],
    ]),
    kind: "folder",
    path: "/Users/ada/Documents/Clients",
  } as unknown as Connection;

  test.each(["fr", "en"] as const)("a folder added offers to choose another one (%s)", async (lang) => {
    // GIVEN a folder added and tried
    await inLocale(lang);
    const words = lang === "fr" ? fr : en;

    // WHEN the trial that answered is rendered
    const { body } = renderToString(TrialSucceeded, {
      props: { kind: "folder", name: "Clients", tools: null, folder: FOLDER, onchooseanother: noop },
    });

    // THEN the system picker can be opened again from there
    expect(body).toContain('data-testid="folder-connection-choose-another"');
    expect(visible(body)).toContain(words.connections.folder.choose_another);
  });

  test("the flow opens the picker again from the trial and from a failure of a folder", () => {
    // GIVEN the flow
    const flowSource = source("./AddFlow.svelte");

    // WHEN what it gives the trial of a folder, and its footer on a failure, are read
    // THEN both lead to the picker of the folder step
    expect(flowSource).toContain('onchooseanother={entry.kind === "folder" ? () => void pickFolder() : undefined}');
    expect(flowSource).toMatch(
      /entry\.kind === "folder"\}\s*<Button[^>]*onclick=\{\(\) => void pickFolder\(\)\}[^>]*data-testid="folder-connection-choose-another"/,
    );
  });

  test.each(["fr", "en"] as const)("a mailbox added says it is only read (%s)", async (lang) => {
    // GIVEN a mailbox added, then another kind of connection
    await inLocale(lang);
    const words = lang === "fr" ? fr : en;

    // WHEN the trial of each is rendered
    const mailbox = renderToString(TrialSucceeded, { props: { kind: "mailbox", name: "ada@example.com", tools: null } }).body;
    const address = renderToString(TrialSucceeded, { props: { kind: "address", name: "Tools", tools: 3 } }).body;

    // THEN the mailbox says nothing is ever sent from it or deleted, the other does not
    expect(mailbox).toContain('data-testid="add-flow-mailbox-read-only"');
    expect(visible(mailbox).replace(/&#39;/g, "'")).toContain(words.connections.mailbox.read_only);
    expect(address).not.toContain('data-testid="add-flow-mailbox-read-only"');
  });
});

describe("the key typed for a service by its address", () => {
  test("is no longer kept once the service is added, and kept whole before, for a new attempt", () => {
    // GIVEN the configuration of an address with the key the service asked for
    const built = addressConfig({ ...EMPTY_ADDRESS_FORM, url: "https://tools.example.com/connect", key: "k-123" }, false);
    if (built.kind !== "ok") throw new Error("the address must build");

    // WHEN the flow keeps what it needs once the service is added
    const kept = keptOnceAdded(built.config);

    // THEN the trial and the removal still read its name, the key is gone,
    // and the configuration of the attempt itself still carries it
    expect(kept.name).toBe("tools-example-com");
    expect(kept.url).toBe("https://tools.example.com/connect");
    expect(kept.transport).toBe("streamable-http");
    expect(JSON.stringify(kept)).not.toContain("k-123");
    expect(built.config.env[KEY_HEADER]).toBe("Bearer k-123");
  });

  test("the flow drops it as soon as the addition succeeded, after the attempt that Retry reads", () => {
    // GIVEN the flow
    const flowSource = source("./AddFlow.svelte");
    const addServer = flowSource.slice(flowSource.indexOf("async function addServer("), flowSource.indexOf("// ── Folder"));

    // WHEN the addition of a server is read
    const heldForRetry = addServer.indexOf("serverConfig = config;");
    const dropped = addServer.indexOf("serverConfig = keptOnceAdded(config);");
    const goneOn = addServer.indexOf("added(mcpConnectionId(config.name));");

    // THEN the whole configuration is held while the attempt runs (a refusal
    // returns before the drop, so Retry still has the key), and only what the
    // trial reads is left once the server is added, before the flow goes on
    expect(heldForRetry).toBeGreaterThan(-1);
    expect(dropped).toBeGreaterThan(heldForRetry);
    expect(goneOn).toBeGreaterThan(dropped);
    expect(addServer.slice(heldForRetry, dropped)).toContain('phase = "failed";');
  });
});

describe("a provider whose own client is missing", () => {
  test("the sign-in of the flow leads to the card of the provider, where its client is entered", () => {
    // GIVEN the screen, where the sign-in dialog of the flow says the client is missing
    const route = source("../../../routes/Connections.svelte");

    // WHEN what it does on "open the client" is read
    const handler = route.slice(route.indexOf("onopenclient={(provider) => {"), route.indexOf("<AddConnectionWindow"));

    // THEN the window gives way and the card of that provider is selected
    // before its client section is brought into view: with no account yet,
    // nothing else opens it
    expect(handler).toContain("addWindow = flowDone();");
    expect(handler.indexOf('select({ kind: "native", provider });')).toBeGreaterThan(-1);
    expect(handler.indexOf('select({ kind: "native", provider });')).toBeLessThan(handler.indexOf("openClientSection(provider);"));
  });
});
