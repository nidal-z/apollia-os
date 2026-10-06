import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, test, vi } from "vitest";
import { render } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

// The theme store reads the browser storage at import; node has none.
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
import PolicyPanel from "./PolicyPanel.svelte";
import { policySummary, type PolicySummary } from "./authorizations";

// The organisation policy has no entry in the sidebar: this panel, at the head
// of the authorizations, is where it is read and where its own view opens. The
// panel is rendered through Svelte's server renderer, the only renderer the
// node environment of this suite offers.

const NONE = policySummary({ state: "none", dir: "/etc/apollia" });
const INVALID = policySummary({
  state: "invalid",
  dir: "/etc/apollia",
  reason: "signature",
  detail: "bad",
});
const ACTIVE: PolicySummary = {
  state: "active",
  deniedFamilies: ["send_external"],
  deniedConnections: ["remote"],
  fixedSettings: [],
  publisher: "IT department",
  signedAt: "2026-09-30T08:00:00Z",
  reason: null,
};

async function panel(summary: PolicySummary): Promise<string> {
  locale.set("en");
  await waitLocale("en");
  return render(PolicyPanel, { props: { summary, onOpen: () => {} } }).body;
}

describe("PolicyPanel - the three states of the organisation policy", () => {
  test("with no policy, the panel says so and still opens the policy view", async () => {
    // GIVEN no policy on this machine
    // WHEN the panel is rendered
    const html = await panel(NONE);

    // THEN it names the policy, says there is none, and offers its view
    expect(html).toContain("Organization policy");
    expect(html).toContain('data-state="none"');
    expect(html).toContain("No policy: you are free.");
    expect(html).toContain('data-testid="authorizations-policy-open"');
    expect(html).not.toContain('data-testid="authorizations-policy-imposed"');
  });

  test("with an active policy, the panel names who signed it and what it imposes", async () => {
    // GIVEN a signed policy that forbids a family and remote connections
    // WHEN the panel is rendered
    const html = await panel(ACTIVE);

    // THEN it shows the signer, what is imposed, and offers its view
    expect(html).toContain('data-state="active"');
    expect(html).toContain("Signed by IT department");
    expect(html).toContain("1 forbidden family");
    expect(html).toContain('data-testid="authorizations-policy-imposed"');
    expect(html).toContain('data-testid="authorizations-policy-open"');
  });

  test("with a refused policy, the panel says what is blocked and opens the detail", async () => {
    // GIVEN a policy deposited on this machine and refused
    // WHEN the panel is rendered
    const html = await panel(INVALID);

    // THEN it says the policy is refused, in the danger tone, and offers the detail
    expect(html).toContain('data-state="invalid"');
    expect(html).toContain("The policy deposited on this machine is refused.");
    expect(html).toContain("text-destructive");
    expect(html).toContain('data-testid="authorizations-policy-open"');
    expect(html).not.toContain('data-testid="authorizations-policy-imposed"');
  });

  test("the panel sits at the head of the authorizations and opens the policy view", () => {
    // GIVEN the source of the authorizations screen
    const screen = readFileSync(
      fileURLToPath(new URL("./AuthorizationsScreen.svelte", import.meta.url)),
      "utf-8",
    );

    // WHEN the panel and the first control of the screen are located
    const layout = screen.indexOf("<PageLayout");
    const policy = screen.indexOf("<PolicyPanel");
    const search = screen.indexOf("<Input");

    // THEN the panel is the first thing under the title, and leads to the policy
    expect(layout).toBeGreaterThan(-1);
    expect(policy).toBeGreaterThan(layout);
    expect(search).toBeGreaterThan(policy);
    expect(screen).toContain('<PolicyPanel summary={auth.policy} onOpen={() => navigateTo("policy")} />');
  });
});
