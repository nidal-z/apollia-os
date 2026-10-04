import { describe, test, expect, vi } from "vitest";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
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
import ConnectorWizard from "./ConnectorWizard.svelte";
import ConnectionRefusal from "./ConnectionRefusal.svelte";
import { REMOTE_DENIED_REASON } from "$lib/integrations/addConnection";
import type { RegistryServerView } from "$lib/types";

const HERE = fileURLToPath(new URL(".", import.meta.url));
const SRC = join(HERE, "../..");

function source(rel: string): string {
  return readFileSync(join(HERE, rel), "utf8");
}

/** Every source file under `dir`, recursively. */
function sources(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return sources(path);
    return /\.(ts|svelte)$/.test(name) && !name.endsWith(".test.ts") ? [path] : [];
  });
}

const NOTION: RegistryServerView = {
  name: "com.notion/mcp",
  title: "Notion",
  description: "Pages and databases",
  version: "1.0.0",
  repository_url: null,
  website_url: null,
  packages: null,
  trust_level: "official",
  category: "productivity",
  enrichment: null,
  is_installed: false,
  remotes: [{ type: "streamable-http", url: "https://mcp.notion.com/mcp", headers: [] }],
} as unknown as RegistryServerView;

async function french(): Promise<void> {
  locale.set("fr");
  await waitLocale("fr");
}

describe("the add dialog", () => {
  test("opens on one warning, without boxes to tick, and one button to add", async () => {
    // GIVEN the add dialog opened on an entry of the selection
    await french();

    // WHEN it is rendered
    const { body } = renderToString(ConnectorWizard, {
      props: { server: NOTION, open: true, onclose: () => {}, oncomplete: () => {} },
    });

    // THEN the warning shows its three sentences and the journal note
    expect(body).toContain('data-testid="wizard-step-disclaimer"');
    expect(body).toContain(fr.integrations.disclaimer.title);
    expect(body).toContain(fr.integrations.disclaimer.journal);
    // AND no box is ticked to accept it, the second gesture is the button
    expect(body).not.toContain('type="checkbox"');
    expect(body).toContain('data-testid="wizard-warning-accept"');
    expect(body).toContain(fr.integrations.wizard.confirm_button);
  });

  test("has no step to test the connection or to choose its approval level", () => {
    // GIVEN the sources of the dialog and of what it shows after the addition
    const wizard = source("ConnectorWizard.svelte");
    const coaching = source("WizardStepCoaching.svelte");

    // WHEN their steps are read
    // THEN no test step nor approval level remains: the trial runs on its own
    expect(wizard).not.toContain("WizardStepTest");
    expect(wizard).not.toContain("Stepper");
    expect(wizard).not.toContain("ApprovalLevelSelector");
    expect(coaching).not.toContain("ApprovalLevelSelector");
    expect(existsSync(join(HERE, "WizardStepTest.svelte"))).toBe(false);
    expect(wizard).toContain("tryAddedConnection");
  });

  test("the separate disclaimer dialog and the two remembered acceptances are gone", () => {
    // GIVEN every source of the interface
    const all = sources(SRC);

    // WHEN they are searched for the dialog and the two storage keys
    const hits = all.filter((path) => {
      const text = readFileSync(path, "utf8");
      return (
        text.includes("McpDisclaimerDialog") ||
        text.includes("apollia-mcp-disclaimer-accepted") ||
        text.includes("apollia-mcp-disclaimer-version")
      );
    });

    // THEN none holds them, and the dialog's file is removed
    expect(hits).toEqual([]);
    expect(existsSync(join(HERE, "McpDisclaimerDialog.svelte"))).toBe(false);
    expect(source("WizardStepDisclaimer.svelte")).not.toContain("localStorage");
  });
});

describe("the refusal box", () => {
  test("says the organisation refused, why, and leads to its policy", async () => {
    // GIVEN a refusal of an online connection by the organisation policy
    await french();
    const refusal = { code: "remote_denied" as const, by: "organisation" as const, reason: REMOTE_DENIED_REASON };

    // WHEN the box is rendered
    const { body } = renderToString(ConnectionRefusal, { props: { refusal, onopen: () => {} } });

    // THEN it names who refused, the reason in plain words, and its action
    expect(body).toContain(fr.connections.refusal.organisation_title);
    expect(body).toContain(fr.connections.refusal.remote_denied);
    expect(body).toContain(fr.connections.refusal.nothing_installed);
    expect(body).toContain(fr.connections.refusal.ask_administrator);
    expect(body).toContain(fr.connections.refusal.open_policy);
    expect(body).not.toContain(REMOTE_DENIED_REASON);
  });

  test("a refusal by the person's permissions leads to the permissions", async () => {
    // GIVEN a refusal by the permissions
    await french();
    const refusal = { code: "remote_denied" as const, by: "permissions" as const, reason: "" };

    // WHEN the box is rendered
    const { body } = renderToString(ConnectionRefusal, { props: { refusal, onopen: () => {} } });

    // THEN it says so and leads to the permissions, with no administrator to ask
    expect(body).toContain(fr.connections.refusal.permissions_title);
    expect(body).toContain(fr.connections.refusal.open_permissions);
    expect(body).not.toContain(fr.connections.refusal.ask_administrator);
  });
});

describe("the component that Node.js is", () => {
  test("is named only in developer mode", () => {
    // GIVEN the two catalogues and the dialog source
    const wizard = source("ConnectorWizard.svelte");

    // WHEN the download message is read in each mode
    // THEN the default text says a required component, the developer text names Node.js
    expect(fr.integrations.wizard.component_downloading).not.toContain("Node");
    expect(fr.dev.integrations.wizard.node_downloading).toContain("Node.js");
    expect(wizard).toContain('$developerMode\n              ? $t("dev.integrations.wizard.node_downloading")');
  });
});
