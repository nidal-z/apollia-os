import { describe, test, expect, vi } from "vitest";
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
import { developerMode } from "$lib/stores/developerMode";
import { tools, type ToolStatusDto } from "$lib/stores/toolGovernance";
import Connections from "../../routes/Connections.svelte";
import ConnectionList from "./list/ConnectionList.svelte";
import BuiltinToolsDetail from "./detail/BuiltinToolsDetail.svelte";

// The tools Apollia ships live on the Connections screen, as a source of their
// own after the connections. Rendered through Svelte's server renderer, the
// only renderer of this suite: effects do not run, so the tool list is put in
// its store before the render.

const TOOLS: ToolStatusDto[] = [
  { name: "web_search", enabled: true, config: { backend: "brave" }, credential_keys: ["brave.api_key"], active_backend: "brave" },
  { name: "web_read", enabled: true, config: null, credential_keys: [], active_backend: null },
  { name: "bash_executor", enabled: false, config: null, credential_keys: [], active_backend: null },
];

async function render(component: unknown, opts: { lang: string; developer: boolean; props?: Record<string, unknown> }): Promise<string> {
  locale.set(opts.lang);
  await waitLocale(opts.lang);
  developerMode.set(opts.developer);
  tools.set(TOOLS);
  return renderToString(component as never, { props: (opts.props ?? {}) as never }).body;
}

const noop = () => {};

describe("the built-in tools in Connections", () => {
  test("the Connections screen lists the Apollia source of built-in tools", async () => {
    // GIVEN three built-in tools, two of them on
    // WHEN the Connections screen is rendered in French
    const html = await render(Connections, { lang: "fr", developer: false });

    // THEN its list carries the section and the entry of the built-in tools, with their count
    expect(html).toContain('data-testid="connections-builtin-section"');
    expect(html).toContain('data-testid="connections-builtin-tools"');
    expect(html).toContain(fr.connections.builtin_section);
    expect(html).toContain(fr.connections.builtin_name);
    expect(html).toContain(
      fr.connections.builtin_count.replace("{active}", "2").replace("{total}", "3"),
    );
  });

  test("the entry of the built-in tools is the selected row when the page shows it", async () => {
    // GIVEN the list with the built-in tools selected
    // WHEN it is rendered
    const html = await render(ConnectionList, {
      lang: "en",
      developer: false,
      props: { rows: [], query: "", kind: "all", selection: { kind: "builtin" }, loading: false, onselect: noop, onadd: noop, builtinTools: { active: 1, total: 4 } },
    });

    // THEN the entry is shown, with its count, in a section of its own, apart
    // from the line that says no connection was added yet
    expect(html).toContain("Apollia, built-in tools");
    expect(html).toContain("1 of 4 on");
    expect(html.indexOf('data-testid="connections-builtin-section"')).toBeGreaterThan(
      html.indexOf('data-testid="connections-sidebar-empty"'),
    );
  });

  test("the page of the built-in tools gives each tool its switch and its configuration", async () => {
    // GIVEN three built-in tools
    // WHEN their page is rendered
    const html = await render(BuiltinToolsDetail, { lang: "en", developer: false });

    // THEN each tool has its card and switch, the configurable ones their configure button, and the reload is there
    for (const tool of TOOLS) {
      expect(html).toContain(`data-testid="tool-card-${tool.name}"`);
      expect(html).toContain(`data-testid="tool-toggle-${tool.name}"`);
    }
    expect(html).toContain('data-testid="tool-configure-web_search"');
    expect(html).toContain('data-testid="tool-configure-web_read"');
    expect(html).toContain('data-testid="tools-reload"');
    expect(html).toContain('data-testid="tools-filter"');
  });
});

describe("the contract of a built-in tool", () => {
  test("is absent from the page with the developer mode off", async () => {
    // GIVEN the developer mode off
    // WHEN the page of the built-in tools is rendered
    const html = await render(BuiltinToolsDetail, { lang: "en", developer: false });

    // THEN neither the contract button, nor the identifier, nor the contract panel is in the DOM
    expect(html).not.toContain("tool-inspect-");
    expect(html).not.toContain("tool-id-");
    expect(html).not.toContain("tool-contract-");
    expect(html).not.toContain(">web_search<");
  });

  test("is offered on every tool with the developer mode on", async () => {
    // GIVEN the developer mode on
    // WHEN the page of the built-in tools is rendered
    const html = await render(BuiltinToolsDetail, { lang: "en", developer: true });

    // THEN every tool carries its identifier and the button that opens its contract
    for (const tool of TOOLS) {
      expect(html).toContain(`data-testid="tool-inspect-${tool.name}"`);
      expect(html).toContain(`data-testid="tool-id-${tool.name}"`);
    }
  });
});
