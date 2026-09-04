import { describe, it, expect, beforeEach } from "vitest";
import { locale } from "svelte-i18n";
import { docsUrl, docsUrlFor } from "./docsUrl";
import { humanize } from "$lib/errors/humanize";

/**
 * The documentation site publishes English at the root and French under a
 * `/fr` prefix, page for page. These tests hold the mapping from the interface
 * locale to that prefix, and hold it on the error mapper too, which is the one
 * caller that resolves its links long after its table was built.
 */

describe("docsUrlFor", () => {
  it("leaves the English routes at the root of the site", () => {
    // GIVEN the interface running in English
    const active = "en";

    // WHEN a help-center route is resolved
    const url = docsUrlFor(active, "/operator-help/agents/install-an-agent");

    // THEN it carries no locale segment, because English is the site default
    expect(url).toBe("https://docs.apollia.fr/operator-help/agents/install-an-agent");
  });

  it("prefixes the French routes with the locale segment", () => {
    // GIVEN the interface running in French
    const active = "fr";

    // WHEN the same route is resolved
    const url = docsUrlFor(active, "/operator-help/agents/install-an-agent");

    // THEN it points at the French page of that same route
    expect(url).toBe("https://docs.apollia.fr/fr/operator-help/agents/install-an-agent");
  });

  it("resolves the home page of each locale", () => {
    // GIVEN the two locales and the root path
    // WHEN each is resolved
    const en = docsUrlFor("en", "/");
    const fr = docsUrlFor("fr", "/");

    // THEN each addresses the home page of its own locale
    expect(en).toBe("https://docs.apollia.fr/");
    expect(fr).toBe("https://docs.apollia.fr/fr/");
  });

  it("accepts a region-tagged locale and an unknown one", () => {
    // GIVEN a region-tagged French tag, an unsupported locale and no locale
    // WHEN each is resolved against the same route
    const tagged = docsUrlFor("fr-FR", "/operator-help");
    const unknown = docsUrlFor("de", "/operator-help");
    const absent = docsUrlFor(null, "/operator-help");

    // THEN the region tag still reaches French, and anything else falls back
    // to the site default rather than to a route that does not exist
    expect(tagged).toBe("https://docs.apollia.fr/fr/operator-help");
    expect(unknown).toBe("https://docs.apollia.fr/operator-help");
    expect(absent).toBe("https://docs.apollia.fr/operator-help");
  });
});

describe("docsUrl reads the locale in force", () => {
  beforeEach(() => {
    locale.set("en");
  });

  it("follows a language change without a reload", () => {
    // GIVEN the interface switched to French after the module was loaded
    locale.set("fr");

    // WHEN a route is resolved through the store-reading entry point
    const url = docsUrl("/operator-help/chat/chat-with-your-ai");

    // THEN the link points at the French page
    expect(url).toBe("https://docs.apollia.fr/fr/operator-help/chat/chat-with-your-ai");
  });
});

describe("the error mapper deep-links into the operator's own locale", () => {
  const identity = (key: string): string => key;

  it("sends a French operator to the French permission guide", () => {
    // GIVEN the interface running in French and a policy denial
    locale.set("fr");

    // WHEN the raw error is humanized
    const humanized = humanize("permission denied by policy", identity);

    // THEN the "learn more" link carries the locale segment
    expect(humanized.learn_more_url).toBe(
      "https://docs.apollia.fr/fr/operator-help/control/approve-or-reject-an-action",
    );
  });

  it("sends an English operator to the English permission guide", () => {
    // GIVEN the interface running in English and the same denial
    locale.set("en");

    // WHEN the raw error is humanized
    const humanized = humanize("permission denied by policy", identity);

    // THEN the link stays at the root of the site
    expect(humanized.learn_more_url).toBe(
      "https://docs.apollia.fr/operator-help/control/approve-or-reject-an-action",
    );
  });
});
