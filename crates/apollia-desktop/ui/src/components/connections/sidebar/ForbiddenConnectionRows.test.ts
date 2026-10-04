import { describe, test, expect } from "vitest";
import { render as renderToString } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";
import "$lib/i18n";

import ForbiddenConnectionRows from "./ForbiddenConnectionRows.svelte";
import fr from "$lib/i18n/fr.json";
import en from "$lib/i18n/en.json";
import type { Connection } from "$lib/types";

const forbidden: Connection[] = [
  {
    id: "mcp:fetch",
    kind: "mcp",
    label: "fetch",
    account: null,
    provenance: "custom",
    remote: false,
    official: false,
    state: "forbidden_by_policy",
    tools: [],
  },
  {
    id: "native:google:ada@example.com",
    kind: "native",
    label: "Google",
    account: "ada@example.com",
    provenance: "selection",
    remote: true,
    official: false,
    state: "forbidden_by_policy",
    tools: [],
  },
];

/** Renders the section through Svelte's server renderer, in `lang`. */
async function renderIn(lang: "fr" | "en", connections: Connection[]): Promise<string> {
  locale.set(lang);
  await waitLocale(lang);
  return renderToString(ForbiddenConnectionRows, { props: { connections } }).body;
}

describe("ForbiddenConnectionRows", () => {
  test("marks each forbidden connection in French", async () => {
    // GIVEN a server and an account the policy forbade, the interface in French
    // WHEN the section is rendered
    const html = await renderIn("fr", forbidden);

    // THEN both are listed by name and account, each with the French mark
    expect(html).toContain('data-testid="connections-forbidden-mcp:fetch"');
    expect(html).toContain('data-testid="connections-forbidden-native:google:ada@example.com"');
    expect(html).toContain("ada@example.com");
    expect(html.split(fr.connections.forbidden_by_policy).length - 1).toBe(2);
    expect(html).toContain("Interdites par la politique (2)");
  });

  test("marks them in English", async () => {
    // GIVEN the same connections, the interface in English
    // WHEN the section is rendered
    const html = await renderIn("en", forbidden);

    // THEN the English mark is shown on both
    expect(html.split(en.connections.forbidden_by_policy).length - 1).toBe(3);
    expect(html).toContain("Forbidden by the policy (2)");
  });

  test("shows nothing when the policy forbade nothing", async () => {
    // GIVEN no forbidden connection
    // WHEN the section is rendered
    const html = await renderIn("en", []);

    // THEN no section is drawn
    expect(html).not.toContain("connections-forbidden-section");
  });
});
