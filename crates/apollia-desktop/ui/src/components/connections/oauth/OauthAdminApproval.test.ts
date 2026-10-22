import { describe, test, expect } from "vitest";
import { render as renderToString } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";
import "$lib/i18n";

import OauthAdminApproval from "./OauthAdminApproval.svelte";
import fr from "$lib/i18n/fr.json";
import en from "$lib/i18n/en.json";

const ADDRESS = "https://login.microsoftonline.com/organizations/adminconsent?client_id=c4f95bc5";

/** The messages of the box in one catalogue, the provider filled in. */
function messagesOf(catalogue: typeof en): string[] {
  const c = catalogue.connections;
  return [
    c.oauth_admin_approval_title,
    c.oauth_admin_approval_body.replace("{provider}", "Microsoft"),
    c.oauth_admin_approval_address,
    c.oauth_admin_approval_copy,
    c.oauth_admin_approval_personal,
  ];
}

/** Escapes what the server renderer escapes in text and attributes. */
function escaped(text: string): string {
  return text.replaceAll("&", "&amp;").replaceAll("'", "&#39;");
}

/**
 * Renders the box through Svelte's server renderer, the only renderer the node
 * environment of this suite offers, in `lang`.
 */
async function renderIn(lang: "fr" | "en"): Promise<string> {
  locale.set(lang);
  await waitLocale(lang);
  return renderToString(OauthAdminApproval, {
    props: { providerShort: "Microsoft", address: ADDRESS },
  }).body;
}

describe("OauthAdminApproval", () => {
  test("says in French that the organisation blocks it, with the address to copy", async () => {
    // GIVEN the interface in French
    // WHEN Microsoft's answer that an administrator must approve is rendered
    const html = await renderIn("fr");

    // THEN the box carries the French reason, the approval address in a field
    // to copy, and that a personal account is not concerned, none of the English
    expect(html).toContain('data-testid="oauth-admin-approval"');
    expect(html).toContain('data-testid="oauth-admin-approval-address"');
    expect(html).toContain(`value="${escaped(ADDRESS)}"`);
    for (const message of messagesOf(fr)) {
      const shown = html.includes(message) || html.includes(escaped(message));
      expect(shown, message).toBe(true);
    }
    expect(html).not.toContain(en.connections.oauth_admin_approval_title);
  });

  test("says it in English with the same structure", async () => {
    // GIVEN the interface in English
    // WHEN the same answer is rendered
    const html = await renderIn("en");

    // THEN the English counterpart is shown, with the same address
    expect(html).toContain(`value="${escaped(ADDRESS)}"`);
    for (const message of messagesOf(en)) expect(html).toContain(message);
    expect(html).toContain("Blocked by your organisation");
  });
});
