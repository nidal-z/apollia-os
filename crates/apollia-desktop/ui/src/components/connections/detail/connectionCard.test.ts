import { describe, test, expect, vi, beforeEach } from "vitest";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { render as renderToString } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";

const invoke = vi.hoisted(() => vi.fn(async (..._args: unknown[]): Promise<unknown> => null));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
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
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";
import { developerMode } from "$lib/stores/developerMode";
import { checkConnection, pauseConnection, removeConnection, resumeConnection } from "$lib/ipc/connections";
import ConnectionCard from "./ConnectionCard.svelte";
import InlineConfirm from "$lib/components/ui/inline-confirm/InlineConfirm.svelte";
import type { Connection } from "$lib/types";

function connection(over: Partial<Connection> = {}): Connection {
  return {
    id: "native:google:ada@example.com",
    kind: "native",
    label: "Google",
    account: "ada@example.com",
    provenance: "selection",
    remote: true,
    official: false,
    state: "needs_reauth",
    status: "needs_repair",
    reason: { key: "sign_in_again", detail: "invalid_grant: token revoked" },
    checkedAt: new Date(Date.now() - 2 * 3_600_000).toISOString(),
    package: null,
    tools: [{ id: "gmail.send", family: "send_external", familySource: "builtin", prechecked: false }],
    ...over,
  };
}

async function renderCard(c: Connection, developer: boolean): Promise<string> {
  locale.set("en");
  await waitLocale("en");
  developerMode.set(developer);
  const noop = () => {};
  return renderToString(ConnectionCard, {
    props: { connection: c, label: "Google", onupdated: noop, onremoved: noop, onreload: noop, onrepair: noop },
  }).body;
}

function visibleText(html: string): string {
  return html.replace(/<!--[\s\S]*?-->/g, " ").replace(/<[^>]+>/g, " ").replace(/\s+/g, " ");
}

function source(path: string): string {
  return readFileSync(fileURLToPath(new URL(path, import.meta.url)), "utf8");
}

beforeEach(() => invoke.mockClear());

describe("the card says whether the connection works", () => {
  test("its state, since when, why not, and the one action that repairs it", async () => {
    // GIVEN an account that must sign in again, found two hours ago
    // WHEN its card is rendered, developer mode off
    const html = await renderCard(connection(), false);
    const text = visibleText(html);

    // THEN the card says the state, the reason and the date, and offers Sign in again and Check
    expect(text).toContain("Needs repair");
    expect(text).toContain("the account must sign in again");
    expect(text).toContain("checked 2 h ago");
    expect(html).toContain('data-repair="sign_in"');
    expect(html.match(/data-testid="connection-repair-btn"/g)).toHaveLength(1);
    expect(html).toContain('data-testid="connection-check-btn"');
    // AND neither the technical detail nor the reason's raw text reaches the user
    expect(html).not.toContain('data-testid="connection-technical"');
    expect(text).not.toContain("invalid_grant");
  });

  test("a working connection offers no repair, a paused one Resume, a blocked one See the rule", async () => {
    // GIVEN a working, a paused and a blocked connection
    const working = await renderCard(connection({ status: "working", reason: null }), false);
    const paused = await renderCard(connection({ status: "paused", reason: null }), false);
    const blocked = await renderCard(connection({ status: "blocked", reason: { key: "forbidden_by_policy" } }), false);

    // WHEN their cards are read
    // THEN the gestures follow the state
    expect(working).not.toContain('data-testid="connection-repair-btn"');
    expect(working).toContain('data-testid="connection-pause-btn"');
    expect(paused).toContain('data-testid="connection-resume-btn"');
    expect(paused).not.toContain('data-testid="connection-pause-btn"');
    expect(blocked).toContain('data-repair="rule"');
    expect(blocked).not.toContain('data-testid="connection-pause-btn"');
    expect(blocked).not.toContain('data-testid="connection-resume-btn"');
  });
});

describe("the gestures of the card reach the runtime", () => {
  test("Check, Pause, Resume and Remove call their commands for the connection", async () => {
    // GIVEN the identifier of a connection
    const id = "imap:ada@example.com";

    // WHEN each gesture's call is made
    await checkConnection(id);
    await pauseConnection(id);
    await resumeConnection(id);
    await removeConnection(id, true);
    await removeConnection(id, false);

    // THEN each command is named with the connection, the removal asked dry first
    expect(invoke.mock.calls).toEqual([
      ["check_connection", { connectionId: id }],
      ["pause_connection", { connectionId: id }],
      ["resume_connection", { connectionId: id }],
      ["remove_connection", { connectionId: id, dryRun: true }],
      ["remove_connection", { connectionId: id, dryRun: false }],
    ]);
  });
});

describe("removing is confirmed in the page", () => {
  test("the confirmation lists what goes, with Cancel and Remove", async () => {
    // GIVEN the confirmation in the page
    locale.set("en");
    await waitLocale("en");

    // WHEN it is rendered
    const html = renderToString(InlineConfirm, {
      props: {
        message: "Remove Google? This goes with it:",
        confirmLabel: "Remove the connection",
        onconfirm: () => {},
        oncancel: () => {},
        "data-testid": "connection-remove-confirm",
      },
    }).body;

    // THEN it carries its message and both actions
    expect(html).toContain('data-testid="connection-remove-confirm-cancel"');
    expect(html).toContain('data-testid="connection-remove-confirm-btn"');
    expect(visibleText(html)).toContain("Remove Google? This goes with it:");
  });

  test("no native dialog is left on the Connections page or its cards", () => {
    // GIVEN the route and every component of the cards
    const dir = fileURLToPath(new URL(".", import.meta.url));
    const files = readdirSync(dir).filter((f) => f.endsWith(".svelte"));
    const sources = [source("../../../routes/Connections.svelte"), ...files.map((f) => source(`./${f}`))];

    // WHEN their code is read
    // THEN none asks through confirm()
    for (const code of sources) expect(code).not.toMatch(/\bconfirm\(/);
  });
});

describe("the technical detail is a section of the developer mode", () => {
  test("it gives the provenance, the reason's detail and the account's OAuth client", async () => {
    // GIVEN an account in need of repair, developer mode on
    // WHEN its card is rendered
    const html = await renderCard(connection(), true);

    // THEN the technical section holds the provenance, the raw reason and the client of the provider
    expect(html).toContain('data-testid="connection-technical"');
    expect(html).toContain('data-testid="connection-detail-reason"');
    expect(visibleText(html)).toContain("invalid_grant");
    expect(html).toContain('data-testid="native-client-google"');
  });
});

describe("the former pages are gone", () => {
  test("their files are removed, and nothing promises a managed OAuth or Add via Google", () => {
    // GIVEN the pages the card replaces
    const former = [
      "McpServerDetail",
      "McpDetailHeader",
      "McpOverviewTab",
      "McpSettingsTab",
      "InactiveServerDetail",
      "ConnectionSummaryDetail",
      "NativeConnectorDetail",
      "NativeAccountsTab",
      "CapabilityMatrix",
    ];

    // WHEN the tree and the catalogues are read
    const english = JSON.stringify(en);
    const promises = (catalogue: typeof en) => {
      const connections = catalogue.connections as Record<string, unknown>;
      return ["native_section_tag", "capabilities"].filter((key) => key in connections);
    };

    // THEN no former page is left, and neither promise is said in either language
    for (const name of former) {
      expect(existsSync(fileURLToPath(new URL(`./${name}.svelte`, import.meta.url)))).toBe(false);
    }
    expect(english).not.toMatch(/managed OAuth|Add via Google/);
    expect(promises(en)).toEqual([]);
    expect(promises(fr as typeof en)).toEqual([]);
  });
});
