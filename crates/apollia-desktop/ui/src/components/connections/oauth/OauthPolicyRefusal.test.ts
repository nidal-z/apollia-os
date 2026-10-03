import { describe, test, expect } from "vitest";
import { render as renderToString } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";
import "$lib/i18n";

import OauthPolicyRefusal from "./OauthPolicyRefusal.svelte";
import fr from "$lib/i18n/fr.json";
import en from "$lib/i18n/en.json";

/** The three messages of the box in one catalogue, the provider filled in. */
function messagesOf(catalogue: typeof en): string[] {
  const c = catalogue.connections;
  return [
    c.oauth_policy_refused_title,
    c.oauth_policy_refused_body.replace("{provider}", "Google"),
    c.oauth_policy_refused_contact,
  ];
}

/**
 * Renders the box through Svelte's server renderer, the only renderer the node
 * environment of this suite offers, in `lang`.
 */
async function renderIn(lang: "fr" | "en"): Promise<string> {
  locale.set(lang);
  await waitLocale(lang);
  return renderToString(OauthPolicyRefusal, { props: { providerShort: "Google" } }).body;
}

describe("OauthPolicyRefusal", () => {
  test("says in French that the organisation policy refused the account", async () => {
    // GIVEN the interface in French
    // WHEN the refusal of a Google account is rendered
    const html = await renderIn("fr");

    // THEN the box carries the French title, the provider nothing was sent to,
    // and whom to ask, and none of the English
    expect(html).toContain('data-testid="oauth-policy-refused"');
    for (const message of messagesOf(fr)) expect(html).toContain(message);
    expect(html).not.toContain(en.connections.oauth_policy_refused_title);
  });

  test("says it in English with the same structure", async () => {
    // GIVEN the interface in English
    // WHEN the same refusal is rendered
    const html = await renderIn("en");

    // THEN the English counterpart is shown
    expect(html).toContain('data-testid="oauth-policy-refused"');
    for (const message of messagesOf(en)) expect(html).toContain(message);
    expect(html).toContain("Nothing was sent to Google.");
  });
});
