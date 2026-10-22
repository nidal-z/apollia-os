import { describe, test, expect, vi } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
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
import TryConnectionDialog from "./shared/TryConnectionDialog.svelte";
import NativeConnectorDetail from "./detail/NativeConnectorDetail.svelte";
import McpDetailHeader from "./detail/McpDetailHeader.svelte";
import ConnectionSummaryDetail from "./detail/ConnectionSummaryDetail.svelte";
import type { TryQuestion } from "$lib/connections/tryConnectorPrompt";
import type { Connection } from "$lib/types";

// The "Try" action is rendered through Svelte's server renderer, the only
// renderer the node environment of this suite offers: on the page of a
// connection of each kind, at the end of the addition of a folder, and in its
// own dialog. The route that opens it from the end of every addition is read
// as source, since the runner mounts no route with its effects.

const noop = () => {};

const FOLDER: Connection = {
  id: "folder:f1",
  kind: "folder",
  label: "Factures fournisseurs",
  account: null,
  path: "/Users/ada/Documents/Factures fournisseurs",
  provenance: "selection",
  remote: false,
  official: false,
  state: "connected",
  status: "working",
  reason: null,
  checkedAt: null,
  package: null,
  tools: [{ id: "file_read", family: "read", familySource: "builtin", prechecked: true }],
};

function text(html: string): string {
  return html
    .replace(/<!--[\s\S]*?-->/g, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/\s+/g, " ");
}

async function french(): Promise<void> {
  locale.set("fr");
  await waitLocale("fr");
}

function nativePage(ontry: (() => void) | undefined): string {
  return renderToString(NativeConnectorDetail as never, {
    props: {
      connector: { id: "google", name: "Google", description: "Gmail, Agenda, Drive" },
      accounts: [{ provider: "google", account_id: "ada@example.com" }],
      loading: false,
      error: null,
      tab: "accounts",
      clientStatus: undefined,
      clientError: null,
      onclientretry: noop,
      onconnect: noop,
      ondisconnect: noop,
      ontabchange: noop,
      ontry,
    } as never,
  }).body;
}

function serverHeader(ontry: (() => void) | undefined): string {
  return renderToString(McpDetailHeader as never, {
    props: {
      label: "Notion", meta: "productivity", status: "active", sync: null,
      testing: false, reconnecting: false, ontest: noop, onreconnect: noop, ontry,
    } as never,
  }).body;
}

describe("Try on the page of a connection", () => {
  test("a native account, a server and a folder each offer Try", async () => {
    // GIVEN the page of a native account, of a server and of a folder, in French
    await french();

    // WHEN each is rendered with its Try action
    const native = nativePage(noop);
    const server = serverHeader(noop);
    const folder = renderToString(ConnectionSummaryDetail as never, {
      props: { connection: FOLDER, label: FOLDER.label, ontry: noop } as never,
    }).body;

    // THEN each carries the Try button, under its French name
    expect(native).toContain('data-testid="native-try-google"');
    expect(server).toContain('data-testid="mcp-try-btn"');
    expect(folder).toContain('data-testid="connection-try-btn"');
    for (const html of [native, server, folder]) {
      expect(text(html)).toContain(fr.connections.try.action);
    }
  });

  test("a page with nothing to try offers no Try", async () => {
    // GIVEN a native provider without an account and a server the screen gives no Try
    await french();

    // WHEN they are rendered without the action
    const native = nativePage(undefined);
    const server = serverHeader(undefined);

    // THEN no Try button is drawn
    expect(native).not.toContain("native-try-google");
    expect(server).not.toContain("mcp-try-btn");
  });
});

describe("Try at the end of an addition", () => {
  test("the flow of every addition ends on Try beside Done", () => {
    // GIVEN the source of the one flow of an addition
    const flow = readFileSync(fileURLToPath(new URL("./add/AddFlow.svelte", import.meta.url)), "utf8");

    // WHEN its last step is read
    const trial = flow.slice(flow.indexOf('{:else if phase === "trial"}\n        {#if trialOk}'));

    // THEN Try sits beside Done, on the connection the flow added, whatever its kind
    expect(trial).toContain('data-testid="add-flow-try"');
    expect(trial).toContain("ontry(id, name)");
    expect(trial).toContain('data-testid="wizard-done-btn"');
  });

  test("the route opens Try at the end of every addition and from every page", () => {
    // GIVEN the source of the Connections route
    const source = readFileSync(
      fileURLToPath(new URL("../../routes/Connections.svelte", import.meta.url)),
      "utf8",
    );

    // WHEN its wiring is read
    // THEN a native account, a folder and a server reach Try at the end of their addition
    expect(source).toMatch(/<NativeOauthDialog[\s\S]*?ontry=\{\(provider, accountId\)/);
    expect(source).toMatch(/<AddConnectionWindow[\s\S]*?ontry=\{\(connectionId, name\)/);
    expect(source).toMatch(/ontryprompt=\{\(connectionId, prompt\) => void launchTry\(connectionId, prompt\)\}/);
    // AND every page passes its Try to the dialog, by the identifier of its connection
    expect(source).toMatch(/<NativeConnectorDetail[\s\S]*?ontry=\{nativeTry\(/);
    expect(source).toMatch(/<McpServerDetail[\s\S]*?ontry=\{\(name\) => openTry\(`mcp:/);
    expect(source).toMatch(/<ConnectionSummaryDetail[\s\S]*?ontry=\{\(\) => openTry\(selectedConnection\.id/);
  });
});

describe("the Try dialog", () => {
  const examples: TryQuestion[] = [
    { id: "notion-search", label: "chercher des pages", question: "Quelles pages parlent du budget ?", general: false },
    { id: "notion-fetch", label: "lire une page", question: "Résume ma dernière page.", general: false },
  ];

  test("offers one question per example, a field of one's own, and launches", async () => {
    // GIVEN a server whose selection entry carries two examples, in French
    await french();

    // WHEN the dialog is rendered
    const html = renderToString(TryConnectionDialog as never, {
      props: { open: true, name: "Notion", questions: examples, onclose: noop, onlaunch: noop } as never,
    }).body;
    const words = text(html);

    // THEN the title names the connection and says the trial is limited to its tools
    expect(words).toContain(fr.connections.try.title.replace("{name}", "Notion"));
    expect(words).toContain(fr.connections.try.subtitle);
    // AND each example is offered under the connection's name, with its question
    expect(words).toContain("Notion · chercher des pages");
    expect(words).toContain("« Quelles pages parlent du budget ? »");
    expect(html).toContain('data-testid="try-connection-question-notion-fetch"');
    // AND the operator may ask their own, then launch
    expect(html).toContain('data-testid="try-connection-own"');
    expect(words).toContain(fr.connections.try.launch);
    expect(html).not.toContain('data-testid="try-connection-no-example"');
  });

  test("a connection without example is tried with the general question", async () => {
    // GIVEN a folder, which carries no example, in French
    await french();
    const general: TryQuestion[] = [
      {
        id: "general",
        label: fr.connections.try.general_label,
        question: fr.connections.try.general_question.replace("{name}", "Factures fournisseurs"),
        general: true,
      },
    ];

    // WHEN the dialog is rendered
    const html = renderToString(TryConnectionDialog as never, {
      props: { open: true, name: "Factures fournisseurs", questions: general, onclose: noop, onlaunch: noop } as never,
    }).body;
    const words = text(html);

    // THEN the general question is the one offered, and the dialog says why
    expect(words).toContain(fr.connections.try.general_label);
    expect(words).toContain("Que puis-je faire avec Factures fournisseurs ?");
    expect(html).toContain('data-testid="try-connection-no-example"');
  });
});
