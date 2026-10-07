import { describe, test, expect, vi } from "vitest";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

import {
  BRIDGE_SETTINGS_SECTION,
  DEFAULT_DEVELOPER_VIEW,
  DEVELOPER_PANES,
  DEVELOPER_VIEWS,
  openBridgeSettings,
  panesOf,
  screenVisible,
} from "./developerScreen";

const HERE = __dirname;
const SRC = resolve(HERE, "../..");

function source(rel: string): string {
  return readFileSync(resolve(SRC, rel), "utf8");
}

/** The component that holds each pane. */
const PANE_FILES = {
  server: "LocalServerPane.svelte",
  documentation: "DocumentationPane.svelte",
  logs: "LogsPane.svelte",
} as const;

describe("the developer screen - panes", () => {
  test("the screen has three panes, each in a component of its own", () => {
    // GIVEN the panes of the screen
    const panes = [...DEVELOPER_PANES];

    // WHEN the component of each is looked up on disk and in the screen
    const screen = source("routes/Developer.svelte");
    const files = panes.map((pane) => PANE_FILES[pane]);
    const missing = files.filter((file) => !existsSync(resolve(HERE, file)));
    const unmounted = files.filter((file) => !screen.includes(`components/developer/${file}`));

    // THEN there are three, in three distinct files the screen mounts
    expect(panes).toEqual(["server", "documentation", "logs"]);
    expect(new Set(files).size).toBe(3);
    expect(missing).toEqual([]);
    expect(unmounted).toEqual([]);
  });

  test("the screen opens on the local server, with its logs under it", () => {
    // GIVEN the screen as it opens
    const view = DEFAULT_DEVELOPER_VIEW;

    // WHEN its panes are read, then those of the documentation
    const opened = panesOf(view);
    const documentation = panesOf("documentation");

    // THEN the server and the logs show first, the documentation alone after
    expect(view).toBe("server");
    expect(opened).toEqual(["server", "logs"]);
    expect(documentation).toEqual(["documentation"]);
    expect(DEVELOPER_VIEWS.map((entry) => entry.view)).toEqual(["server", "documentation"]);
  });

  test("the screen renders nothing with the developer mode off, and itself with it on", () => {
    // GIVEN the two positions of the developer mode
    const off = false;
    const on = true;

    // WHEN the screen decides whether it renders
    const shownOff = screenVisible(off);
    const shownOn = screenVisible(on);

    // THEN it is empty with the mode off, and the screen reads that decision
    expect(shownOff).toBe(false);
    expect(shownOn).toBe(true);
    expect(source("routes/Developer.svelte")).toContain("{#if screenVisible($developerMode)}");
  });
});

describe("the developer screen - what stays in the settings", () => {
  test("the link opens the settings on the section of the mode and the bridge", () => {
    // GIVEN the opener of the settings window
    const open = vi.fn();

    // WHEN the link of the column is followed
    openBridgeSettings(open);

    // THEN the settings open on the developer section, which holds the bridge
    expect(BRIDGE_SETTINGS_SECTION).toBe("developer");
    expect(open).toHaveBeenCalledWith("developer");
    expect(source("routes/settings/Developer.svelte")).toContain("<McpServerSection");
  });

  test("the screen repeats neither the mode switch nor the bridge", () => {
    // GIVEN every component of the screen
    const files = [
      "routes/Developer.svelte",
      "components/developer/DeveloperColumn.svelte",
      "components/developer/ServerBar.svelte",
      "components/developer/LoadModelButton.svelte",
      ...Object.values(PANE_FILES).map((file) => `components/developer/${file}`),
    ];

    // WHEN each is searched for the switch of the mode and the bridge section
    const repeating = files.filter((file) => {
      const text = source(file);
      return (
        text.includes("setDeveloperMode") ||
        text.includes("toggleDeveloperMode") ||
        text.includes("McpServerSection")
      );
    });

    // THEN none carries them, and the column carries the link
    expect(repeating).toEqual([]);
    expect(source("components/developer/DeveloperColumn.svelte")).toContain(
      "openBridgeSettings(openSettingsWindow)",
    );
  });
});

describe("the developer screen - the loading window", () => {
  test("the button that loads a model mounts the loading window of the chat", () => {
    // GIVEN the button of the bar
    const button = source("components/developer/LoadModelButton.svelte");

    // WHEN its markup is read
    const mounts = button.includes("<ModelLoadDialog />");
    const asks = button.includes("askModelLoad(loadAsk(");

    // THEN it asks the shared window to open, and mounts that same window
    expect(mounts).toBe(true);
    expect(asks).toBe(true);
    expect(button).toContain('from "../chat/ModelLoadDialog.svelte"');
  });
});
