// The developer screen: its three panes, the two views of its column, and
// where it sends for what it does not hold.
//
// The decisions live here, apart from the components, so Vitest can hold them
// without rendering anything.

import type { SettingsSubRoute } from "$lib/stores/settings";

/** A pane of the screen. */
export type DeveloperPane = "server" | "documentation" | "logs";

/** The three panes, in reading order. */
export const DEVELOPER_PANES: readonly DeveloperPane[] = ["server", "documentation", "logs"];

/** A view the column switches between. The logs sit under the local server. */
export type DeveloperView = "server" | "documentation";

/** The entries of the column, in order. */
export const DEVELOPER_VIEWS: readonly {
  view: DeveloperView;
  labelKey: string;
  testid: string;
}[] = [
  { view: "server", labelKey: "dev.developer.views.server", testid: "developer-view-server" },
  {
    view: "documentation",
    labelKey: "dev.developer.views.documentation",
    testid: "developer-view-documentation",
  },
];

/** The view the screen opens on. */
export const DEFAULT_DEVELOPER_VIEW: DeveloperView = "server";

/** The panes on screen for `view`: the logs go with the local server. */
export function panesOf(view: DeveloperView): DeveloperPane[] {
  return view === "server" ? ["server", "logs"] : ["documentation"];
}

/**
 * Whether the screen renders anything: never with the developer mode off,
 * even if a caller mounts it past the router.
 */
export function screenVisible(developerMode: boolean): boolean {
  return developerMode;
}

/**
 * The section of the settings that keeps the developer mode switch and the
 * MCP bridge: neither is repeated on this screen, which links there.
 */
export const BRIDGE_SETTINGS_SECTION: SettingsSubRoute = "developer";

/** Opens the settings on the section of the developer mode and the bridge. */
export function openBridgeSettings(open: (section: string) => unknown): void {
  open(BRIDGE_SETTINGS_SECTION);
}
