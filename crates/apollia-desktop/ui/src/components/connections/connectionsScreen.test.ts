import { describe, test, expect, vi } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { render as renderToString } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

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
import { developerMode } from "$lib/stores/developerMode";
import { connectionRows } from "$lib/connections/connectionRows";
import Connections from "../../routes/Connections.svelte";
import ConnectionList from "./list/ConnectionList.svelte";
import NativeConnectorDetail from "./detail/NativeConnectorDetail.svelte";
import McpDetailHeader from "./detail/McpDetailHeader.svelte";
import McpToolsTab from "./detail/McpToolsTab.svelte";
import McpOverviewTab from "./detail/McpOverviewTab.svelte";
import McpSettingsTab from "./detail/McpSettingsTab.svelte";
import ConnectionSummaryDetail from "./detail/ConnectionSummaryDetail.svelte";
import BuiltinToolsDetail from "./detail/BuiltinToolsDetail.svelte";
import AddConnectionWindow from "./catalogue/AddConnectionWindow.svelte";
import CustomMcpForm from "./catalogue/CustomMcpForm.svelte";
import { settingsGroups } from "../integrations/McpServerSettingsEditor.svelte";
import type { Connection } from "$lib/types";

// Every screen of Connections is rendered through Svelte's server renderer, the
// only renderer the node environment of this suite offers, once with the
// developer mode off and once on. Effects do not run there: what a screen
// fetches over IPC is handed to it as props, and the settings editor, which
// fetches its own configuration, is covered through the helper that decides its
// field groups.

function connection(over: Partial<Connection> = {}): Connection {
  return {
    id: "mcp:linear",
    kind: "mcp",
    label: "linear",
    account: null,
    provenance: "registry",
    remote: true,
    official: true,
    state: "connected",
    status: "working",
    reason: null,
    checkedAt: null,
    package: null,
    tools: [{ id: "mcp:linear/list_issues", family: "read", familySource: "server", prechecked: true }],
    ...over,
  };
}

const CONNECTIONS: Connection[] = [
  connection({ id: "native:google:ada@example.com", kind: "native", label: "Google", account: "ada@example.com", provenance: "selection", official: false }),
  connection(),
  connection({ id: "mcp:files", label: "files", remote: false, provenance: "selection", official: false }),
  connection({ id: "folder:invoices", kind: "folder", label: "Invoices", remote: false, provenance: "custom", official: false, tools: [] }),
];

const TOOLS = [
  {
    full_name: "linear__list_issues",
    local_name: "list_issues",
    description: "List the issues of a team",
    input_schema: { type: "object", properties: { team: { type: "string" } } },
  },
];

const CLIENT = {
  provider: "google",
  effective_client_id: "1234.apps.googleusercontent.com",
  source: "file",
  override_client_id: null,
  has_client_secret: true,
  client_secret_source: "keyring",
  requires_client_secret: true,
  has_api_key: false,
  api_key_source: "none",
  requires_api_key: false,
} as never;

type Rendered = { component: unknown; props?: Record<string, unknown> };

/** Every screen of Connections, with what it needs to show its technical parts. */
function screens(): Record<string, Rendered> {
  const rows = connectionRows(CONNECTIONS);
  const noop = () => {};
  return {
    route: { component: Connections },
    list: {
      component: ConnectionList,
      props: { rows, query: "", kind: "all", selection: { kind: "connection", id: rows[1].key }, loading: false, onselect: noop, onadd: noop, builtinTools: { active: 2, total: 3 } },
    },
    native: {
      component: NativeConnectorDetail,
      props: {
        connector: { id: "google", name: "Google Workspace", description: "Gmail, Calendar, Drive" },
        remote: true,
        accounts: [{ provider: "google", account_id: "ada@example.com" }],
        loading: false,
        error: null,
        tab: "accounts",
        clientStatus: CLIENT,
        clientError: null,
        onclientretry: noop,
        onconnect: noop,
        ondisconnect: noop,
        ontabchange: noop,
      },
    },
    serverHeader: {
      component: McpDetailHeader,
      props: { label: "Linear", meta: "Project management", remote: true, status: "active", sync: null, testing: false, reconnecting: false, ontest: noop, onreconnect: noop },
    },
    serverOverview: {
      component: McpOverviewTab,
      props: { health: { state: "healthy", verified: true }, toolsCount: 1, statusError: null, lastActivityLabel: null, testState: "idle", testToolCount: 0, testDetail: null, testError: null, reconnectError: null },
    },
    serverTools: { component: McpToolsTab, props: { tools: TOOLS, serverName: "linear" } },
    serverSettings: {
      component: McpSettingsTab,
      props: { serverName: "linear", confirmDisconnect: false, disconnecting: false, disconnectError: null, onsaved: noop, onrequestdisconnect: noop, oncanceldisconnect: noop, onconfirmdisconnect: noop },
    },
    builtinTools: { component: BuiltinToolsDetail },
    otherConnection: { component: ConnectionSummaryDetail, props: { connection: CONNECTIONS[3], label: "Invoices" } },
    addSheet: {
      component: AddConnectionWindow,
      props: { open: true, query: "", chosen: null, selection: [], publicEntries: [], failures: [], catalogueTier: "none", loading: true, agents: [], installedNames: new Set(), installedByPackage: new Map(), onclose: noop, onchoose: noop, onback: noop, onexploreall: noop, onbacktoselection: noop, onadd: noop, onmanage: noop, oninstalled: noop },
    },
    addAddressCard: {
      component: AddConnectionWindow,
      props: { open: true, query: "", chosen: "address", selection: [], publicEntries: [], failures: [], catalogueTier: "curated", loading: false, agents: [], installedNames: new Set(), installedByPackage: new Map(), onclose: noop, onchoose: noop, onback: noop, onexploreall: noop, onbacktoselection: noop, onadd: noop, onmanage: noop, oninstalled: noop },
    },
    customConnection: { component: CustomMcpForm, props: { oninstalled: noop } },
  };
}

/** The text a user reads: tags, attributes and scripts removed. */
function visibleText(html: string): string {
  return html
    .replace(/<script[\s\S]*?<\/script>/g, " ")
    .replace(/<!--[\s\S]*?-->/g, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/\s+/g, " ");
}

async function render(name: string, on: boolean): Promise<string> {
  locale.set("en");
  await waitLocale("en");
  developerMode.set(on);
  const { component, props } = screens()[name];
  return renderToString(component as never, { props: (props ?? {}) as never }).body;
}

const SCREENS = Object.keys(screens());

describe("Connections, developer mode off", () => {
  test.each(SCREENS)("the %s screen says neither MCP nor a transport, a command or a schema", async (name) => {
    // GIVEN the developer mode off
    // WHEN the screen is rendered
    const html = await render(name, false);
    const text = visibleText(html);

    // THEN no technical word reaches the user
    expect(text).not.toMatch(/\bMCP\b/);
    expect(text).not.toMatch(/\b(stdio|SSE|streamable-http|Transport|Command)\b/);
    expect(html).not.toContain("mcp-tool-disc-");
    expect(html).not.toContain("custom-mcp-transport-select");
    expect(html).not.toContain("custom-mcp-command-input");
    expect(html).not.toContain("connection-provenance-");
  });
});

describe("Connections, developer mode on", () => {
  test("the list gives each connection's technical kind and origin", async () => {
    // GIVEN the developer mode on
    // WHEN the list is rendered
    const html = await render("list", true);

    // THEN each row carries its provenance line, MCP named on the servers
    expect(html).toContain('data-testid="connection-provenance-mcp:linear"');
    expect(visibleText(html)).toContain("MCP · registry · Official");
    expect(visibleText(html)).toContain("native · selection");
  });

  test("the tools of a server give their provenance and their raw schema", async () => {
    // GIVEN the developer mode on
    // WHEN the tools of a server are rendered
    const html = await render("serverTools", true);

    // THEN each tool names the server it comes from and its full name, and opens on its schema
    expect(visibleText(html)).toContain("MCP server linear · linear__list_issues");
    expect(html).toContain('data-testid="mcp-tool-disc-list_issues"');
    expect(visibleText(html)).toContain("Schema");
  });

  test("the custom connection offers the transport and the command line", async () => {
    // GIVEN the developer mode on
    // WHEN the custom connection form is rendered
    const html = await render("customConnection", true);

    // THEN the transport selector and the command field are there
    expect(html).toContain('data-testid="custom-mcp-transport-select"');
    expect(html).toContain('data-testid="custom-mcp-command-input"');
    expect(visibleText(html)).toMatch(/\bMCP\b/);
  });

  test("the add window names the catalogue by its technical name", async () => {
    // GIVEN the developer mode on
    // WHEN the add window is rendered
    const text = visibleText(await render("addSheet", true));

    // THEN the catalogue and the custom form keep their technical names
    expect(text).toContain("public MCP catalogue");
    expect(text).toContain("Custom MCP");
  });

  test("a mailbox or folder page gives the provenance of the connection and of each tool", async () => {
    // GIVEN the developer mode on and a connection with a tool
    const withTool = { ...CONNECTIONS[3], tools: CONNECTIONS[1].tools };
    locale.set("en");
    await waitLocale("en");
    developerMode.set(true);

    // WHEN its page is rendered
    const html = renderToString(ConnectionSummaryDetail, {
      props: { connection: withTool, label: "Invoices" },
    }).body;

    // THEN the connection and its tool carry their provenance
    expect(html).toContain('data-testid="connection-detail-provenance"');
    expect(html).toContain('data-testid="connection-tool-provenance"');
  });

  test("the settings of a server show the command and the transport only in developer mode", () => {
    // GIVEN a local server and a remote one
    // WHEN the editor decides its field groups in each mode
    const localOff = settingsGroups("stdio", false);
    const localOn = settingsGroups("stdio", true);
    const remoteOff = settingsGroups("streamable-http", false);
    const remoteOn = settingsGroups("sse", true);

    // THEN the command line and the transport wait for the developer mode, the
    // arguments and the address do not
    expect(localOff).toEqual({ launch: false, args: true, url: false, transport: false });
    expect(localOn).toEqual({ launch: true, args: true, url: false, transport: false });
    expect(remoteOff).toEqual({ launch: false, args: false, url: true, transport: false });
    expect(remoteOn).toEqual({ launch: false, args: false, url: true, transport: true });
  });
});

describe("Connections, one list and one word", () => {
  test("the list has no native or MCP section, and the route keeps no hard-coded connector list", async () => {
    // GIVEN the list rendered with an account, two servers and a folder
    const html = await render("list", false);
    const text = visibleText(html);
    const route = readFileSync(
      fileURLToPath(new URL("../../routes/Connections.svelte", import.meta.url)),
      "utf8",
    );
    const header = readFileSync(
      fileURLToPath(new URL("./list/ConnectionListHeader.svelte", import.meta.url)),
      "utf8",
    );

    // WHEN its words and the route source are read
    // THEN the four connections sit in one list, with no section between them
    for (const c of CONNECTIONS) expect(html).toContain(`connection-state-${c.id}`);
    expect(text).not.toContain("Apollia native");
    expect(text).not.toContain("MCP servers");
    // AND adding one is the action of the page header, not a line of the list
    expect(header).toContain('data-testid="connections-open-catalogue"');
    expect(html).not.toContain('data-testid="connections-open-catalogue"');
    expect(route).not.toContain("NATIVE_CONNECTORS");
    expect(route).toContain("listConnections");
  });

  test("the add window offers a service by its address under its plain name", async () => {
    // GIVEN the developer mode off
    // WHEN the add window is rendered
    const html = await render("addSheet", false);
    const text = visibleText(html);

    // THEN it is titled with the official word, offers the native accounts and a service by its address
    expect(text).toContain("Add a connection");
    expect(text).toContain("Connect a service by its address");
    expect(html).toContain('data-testid="catalogue-native-google"');
    expect(html).toContain('data-testid="catalogue-native-microsoft"');
  });

  test("the words of the screen are the same in French", async () => {
    // GIVEN the interface in French and the developer mode off
    locale.set("fr");
    await waitLocale("fr");
    developerMode.set(false);

    // WHEN the add window is rendered
    const { component, props } = screens().addSheet;
    const text = visibleText(renderToString(component as never, { props: (props ?? {}) as never }).body);

    // THEN it uses the French words of the catalogue, and still never says MCP
    expect(text).toContain(fr.connections.add_connection);
    expect(text).toContain(fr.connections.add_window.groups.address);
    expect(text).not.toMatch(/\bMCP\b/);
  });
});

describe("Connections, the remote badge", () => {
  test("a remote connection carries the badge in the list, a local one does not", async () => {
    // GIVEN a remote server and a local one in the list
    // WHEN the list is rendered
    const html = await render("list", false);

    // THEN only the remote one carries the badge
    expect(html).toContain('data-testid="connection-remote-mcp:linear"');
    expect(html).not.toContain('data-testid="connection-remote-mcp:files"');
    expect(html).not.toContain('data-testid="connection-remote-folder:invoices"');
  });

  test("the page of a remote connection carries the badge", async () => {
    // GIVEN a remote server and a remote native account
    // WHEN their pages are rendered
    const server = await render("serverHeader", false);
    const native = await render("native", false);

    // THEN both pages carry it, worded for the user
    expect(server).toContain('data-testid="mcp-detail-remote"');
    expect(native).toContain('data-testid="native-detail-remote"');
    expect(visibleText(server)).toContain("Remote");
  });
});
