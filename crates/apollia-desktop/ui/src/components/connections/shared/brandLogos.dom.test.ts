import { describe, test, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { locale, waitLocale } from "svelte-i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import "$lib/i18n";
import { developerMode } from "$lib/stores/developerMode";
import { brandLogoPath } from "$lib/connections/brandLogo";
import type { ConnectionRow } from "$lib/connections/connectionRows";
import type { Connection, ConnectorEnrichmentView, RegistryServerView } from "$lib/types";
import AddConnectionWindow from "../catalogue/AddConnectionWindow.svelte";
import ConnectionTile from "../list/ConnectionTile.svelte";
import ConnectionCard from "../detail/ConnectionCard.svelte";

// The dialog runs a Svelte transition, which jsdom cannot play.
Element.prototype.animate = function (): Animation {
  const animation = { onfinish: null as (() => void) | null, cancel: () => {}, finished: Promise.resolve() };
  setTimeout(() => animation.onfinish?.(), 0);
  return animation as unknown as Animation;
};

function enrichment(label: string, icon: string): ConnectorEnrichmentView {
  return {
    operator_label: { en: label, fr: label },
    category: "finance",
    icon_name: icon,
    trust_level: "verified_official",
    auth_help_url: null,
    auth_help_text: null,
    auth_help_i18n_key: null,
    oauth_pre_registered_client_id_env: null,
  };
}

function entry(name: string, label: string, icon: string): RegistryServerView {
  return {
    name,
    title: null,
    description: null,
    version: "1.0.0",
    repository_url: null,
    website_url: null,
    packages: null,
    trust_level: "verified_official",
    category: "finance",
    enrichment: enrichment(label, icon),
    is_installed: false,
    remotes: [{ type: "streamable-http", url: `https://${name}.example/mcp`, headers: [] }],
  };
}

const PAYPAL = entry("com.paypal.mcp/mcp", "PayPal", "paypal");
const GOCARDLESS = entry("com.gocardless/gc-mcp", "GoCardless", "landmark");
const noop = () => {};

let target: HTMLElement;
let mounted: ReturnType<typeof mount> | null = null;

async function settle(rounds = 10): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

function byTestid(id: string): Element | null {
  return document.querySelector(`[data-testid="${id}"]`);
}

/** The glyph a logo draws, read from the page. */
function glyphOf(id: string): string | null {
  return byTestid(id)?.querySelector("svg path")?.getAttribute("d") ?? null;
}

function mountWindow(chosen: string | null): void {
  mounted = mount(AddConnectionWindow as never, {
    target,
    props: {
      open: true, query: "", chosen, adding: false, resume: null, selection: [PAYPAL, GOCARDLESS],
      publicEntries: [], failures: [], catalogueTier: "curated", loading: false, agents: [],
      installedNames: new Set(), installedByPackage: new Map(), onclose: noop, onchoose: noop,
      onback: noop, onexploreall: noop, onbacktoselection: noop, onadd: noop, onmanage: noop,
      onadded: noop, ondone: noop, onsignin: noop, ontry: noop, ontryprompt: noop, onopenrule: noop,
    } as never,
  });
}

beforeEach(async () => {
  locale.set("en");
  await waitLocale("en");
  developerMode.set(false);
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (mounted) unmount(mounted);
  mounted = null;
  target.remove();
  document.body.innerHTML = "";
});

describe("the logo of a service, on every surface that names it", () => {
  test("a row of the add window draws the logo of its brand, a brand with none keeps the generic tile", async () => {
    // GIVEN the add window over an entry with a logo and one without
    mountWindow(null);
    await settle();
    // WHEN its rows are read
    // THEN PayPal draws the PayPal glyph, hidden from assistive technology
    //      since its name stands beside it, and GoCardless draws no logo
    expect(glyphOf("catalogue-logo-com.paypal.mcp/mcp")).toBe(brandLogoPath("paypal"));
    expect(byTestid("catalogue-logo-com.paypal.mcp/mcp")?.querySelector("svg")?.getAttribute("aria-hidden")).toBe(
      "true",
    );
    expect(byTestid("catalogue-logo-com.gocardless/gc-mcp")).toBeNull();
    expect(byTestid("catalogue-entry-com.gocardless/gc-mcp")).not.toBeNull();
  });

  test("the card of an entry draws its logo beside its name", async () => {
    // GIVEN the add window with the card of PayPal open
    mountWindow("entry:com.paypal.mcp/mcp");
    await settle();
    // WHEN the card is read
    // THEN it draws the PayPal glyph
    expect(byTestid("catalogue-card")).not.toBeNull();
    expect(glyphOf("catalogue-card-logo")).toBe(brandLogoPath("paypal"));
  });

  test("a connection of the list draws the logo of the entry it came from, another its kind", async () => {
    // GIVEN two rows of the list, one from the PayPal entry and one custom
    const row = (key: string, icon: string | null): ConnectionRow => ({
      key, kind: "mcp", label: key, account: null, path: null, provenance: "selection", remote: true, official: true,
      status: "working", reason: null, checkedAt: null, package: null, toolCount: 0, icon,
    });
    mounted = mount(ConnectionTile as never, {
      target,
      props: { row: row("mcp:com-paypal-mcp-mcp", "paypal"), selected: false, onselect: noop } as never,
    });
    const other = mount(ConnectionTile as never, {
      target,
      props: { row: row("mcp:acme", null), selected: false, onselect: noop } as never,
    });
    await settle();
    // WHEN they are read
    // THEN the first draws the PayPal glyph and the second no logo
    expect(glyphOf("connection-logo-mcp:com-paypal-mcp-mcp")).toBe(brandLogoPath("paypal"));
    expect(byTestid("connection-logo-mcp:acme")).toBeNull();
    unmount(other);
  });

  test("the card of a connection draws the logo of the entry it came from", async () => {
    // GIVEN the card of a connection added from the PayPal entry
    const connection = {
      id: "mcp:com-paypal-mcp-mcp", kind: "mcp", label: "PayPal", account: null, provenance: "selection",
      remote: true, official: true, state: "connected", status: "working", reason: null, checkedAt: null,
      package: null, tools: [],
    } as unknown as Connection;
    mounted = mount(ConnectionCard as never, {
      target,
      props: {
        connection, label: "PayPal", iconName: "paypal", onupdated: noop, onremoved: noop, onreload: noop,
        onrepair: noop,
      } as never,
    });
    await settle();
    // WHEN its header is read
    // THEN it draws the PayPal glyph
    expect(glyphOf("connection-detail-logo")).toBe(brandLogoPath("paypal"));
  });
});
