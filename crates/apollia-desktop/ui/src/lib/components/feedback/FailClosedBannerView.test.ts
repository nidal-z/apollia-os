import { describe, test, expect } from "vitest";
import { render as renderToString } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";
import "$lib/i18n";

import FailClosedBannerView from "./FailClosedBannerView.svelte";
import { failClosedBanner } from "$lib/governance/failClosedBanner";
import { announcesFailClosedChange } from "$lib/stores/governanceFailClosed";
import type { FailClosed } from "$lib/ipc/governance";
import fr from "$lib/i18n/fr.json";
import en from "$lib/i18n/en.json";

const UNREADABLE: FailClosed = { cause: "store_unreadable", detail: "file is not a database" };
const REFUSED: FailClosed = {
  cause: "policy_invalid",
  reason: "signature",
  detail: "the signature does not hold",
};

/** Renders the banner through Svelte's server renderer, in `lang`. */
async function renderIn(lang: "fr" | "en", failClosed: FailClosed | null): Promise<string> {
  locale.set(lang);
  await waitLocale(lang);
  return renderToString(FailClosedBannerView, { props: { failClosed } }).body;
}

/** The server renderer escapes the apostrophe of a text node. */
function shown(text: string): string {
  return text.replaceAll("'", "&#39;");
}

function contains(html: string, text: string): boolean {
  return html.includes(text) || html.includes(shown(text));
}

describe("failClosedBanner", () => {
  test("says nothing while governance decides normally", () => {
    // GIVEN a governance that does not fail closed
    // WHEN the banner is decided
    const banner = failClosedBanner(null);

    // THEN there is no banner
    expect(banner).toBeNull();
  });

  test("offers to read an unreadable database again", () => {
    // GIVEN a permissions database that cannot be read
    // WHEN the banner is decided
    const banner = failClosedBanner(UNREADABLE);

    // THEN it says the database is unreadable and offers to retry
    expect(banner?.reasonKey).toBe("governance.fail_closed.store_unreadable");
    expect(banner?.policyReasonKey).toBeNull();
    expect(banner?.action).toBe("retry");
  });

  test("names the refusal class of a refused policy and opens the policy", () => {
    // GIVEN each refusal class, and one the interface does not know
    const reasons = ["schema", "signature", "key", "owner", "tomorrow"];

    // WHEN the banner of each is decided
    const banners = reasons.map((reason) =>
      failClosedBanner({ cause: "policy_invalid", reason, detail: "" }),
    );

    // THEN each names its class in words, an unknown one as unknown, and
    // offers the policy rather than a retry
    expect(banners.map((b) => b?.policyReasonKey)).toEqual([
      "governance.fail_closed.policy_reason.schema",
      "governance.fail_closed.policy_reason.signature",
      "governance.fail_closed.policy_reason.key",
      "governance.fail_closed.policy_reason.owner",
      "governance.fail_closed.policy_reason.unknown",
    ]);
    for (const banner of banners) {
      expect(banner?.reasonKey).toBe("governance.fail_closed.policy_invalid");
      expect(banner?.action).toBe("view_policy");
    }
  });

  test("follows only the runtime event that announces a change of the state", () => {
    // GIVEN the event the runtime announces, and another one
    // WHEN each is tested
    // THEN only the fail-closed change triggers a new read
    expect(announcesFailClosedChange("GovernanceFailClosed")).toBe(true);
    expect(announcesFailClosedChange("ConfigInvalid")).toBe(false);
  });
});

describe("FailClosedBannerView", () => {
  test("renders nothing while governance decides normally", async () => {
    // GIVEN a governance that does not fail closed
    // WHEN the banner is rendered
    const html = await renderIn("fr", null);

    // THEN no banner is in the page
    expect(html).not.toContain('data-testid="governance-fail-closed-banner"');
  });

  test("says in French that the database is unreadable, reading continues, and offers a retry", async () => {
    // GIVEN the interface in French and an unreadable database
    // WHEN the banner is rendered
    const html = await renderIn("fr", UNREADABLE);

    // THEN it carries the French title, the reason and the retry, none of the English
    const g = fr.governance.fail_closed;
    expect(html).toContain('data-cause="store_unreadable"');
    expect(contains(html, g.title)).toBe(true);
    expect(contains(html, g.store_unreadable)).toBe(true);
    expect(html).toContain('data-testid="governance-fail-closed-retry"');
    expect(contains(html, g.retry)).toBe(true);
    expect(html).not.toContain('data-testid="governance-fail-closed-view-policy"');
    expect(html).not.toContain(en.governance.fail_closed.title);
  });

  test("says in English that the policy is refused and why, and offers the policy", async () => {
    // GIVEN the interface in English and a policy refused for its signature
    // WHEN the banner is rendered
    const html = await renderIn("en", REFUSED);

    // THEN it carries the English title, the reason with its class, and the
    // action that opens the policy
    const g = en.governance.fail_closed;
    expect(html).toContain('data-cause="policy_invalid"');
    expect(contains(html, g.title)).toBe(true);
    expect(
      contains(html, g.policy_invalid.replace("{reason}", g.policy_reason.signature)),
    ).toBe(true);
    expect(html).toContain('data-testid="governance-fail-closed-view-policy"');
    expect(contains(html, g.view_policy)).toBe(true);
    expect(html).not.toContain('data-testid="governance-fail-closed-retry"');
  });

  test("says the refused policy in French with the same structure", async () => {
    // GIVEN the interface in French and the same refused policy
    // WHEN the banner is rendered
    const html = await renderIn("fr", REFUSED);

    // THEN the French sentence names the class in French
    const g = fr.governance.fail_closed;
    expect(
      contains(html, g.policy_invalid.replace("{reason}", g.policy_reason.signature)),
    ).toBe(true);
    expect(contains(html, g.view_policy)).toBe(true);
  });
});
