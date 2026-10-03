/**
 * The settings window: opened above the current screen, closed by the cross or
 * Escape, held open by typed changes not yet saved, reached by the System
 * chords, the help topics and every entry of the application.
 *
 * Components cannot be mounted under this Vitest configuration (node
 * environment, no rendering library), so the stores are driven directly and
 * the markup is read from the sources the bundle is built from.
 */
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, resolve } from "node:path";
import { get } from "svelte/store";
import { currentRoute, navigateTo } from "$lib/stores/navigation";
import {
  settingsWindow,
  settingsWindowOpen,
  closeSettingsWindow,
  consumeSettingsFocus,
  installSettingsCloseGuard,
  requestCloseSettingsWindow,
} from "$lib/stores/settingsWindow";
import {
  settingsSubRoute,
  SETTINGS_SUB_ROUTES,
  DEFAULT_SETTINGS_SUB_ROUTE,
} from "$lib/stores/settings";
import {
  settingsDirtyStore,
  setRouteState,
  holdsExplicitChanges,
} from "$lib/stores/settingsDirty";
import { navigateToSettings } from "$lib/router";
import { SHORTCUTS, comboFor } from "$lib/navigation/shortcuts";
import {
  registerShortcutHandler,
  dispatchShortcutEvent,
} from "$lib/navigation/shortcutDispatcher";
import {
  registerGlobalShortcutHandlers,
  SETTINGS_SECTION_SHORTCUTS,
} from "$lib/navigation/globalShortcutHandlers";
import { SETTINGS_NAV } from "$lib/settings/settingsNav";
import { HELP_TOPIC_TARGETS, openHelpTopic } from "../../components/settings/help/HelpTopics.svelte";

const SRC = resolve(__dirname, "../..");

function source(rel: string): string {
  return readFileSync(resolve(SRC, rel), "utf8");
}

function sourceFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) return sourceFiles(full);
    return /\.(ts|svelte)$/.test(name) && !name.endsWith(".test.ts") ? [full] : [];
  });
}

/** A keydown as the dispatcher reads it, outside any input. */
function chord(key: string, mods: { meta?: boolean; ctrl?: boolean; shift?: boolean }) {
  return {
    key,
    metaKey: mods.meta === true,
    ctrlKey: mods.ctrl === true,
    altKey: false,
    shiftKey: mods.shift === true,
    target: null,
    preventDefault: () => {},
  } as unknown as KeyboardEvent;
}

function reset(): void {
  closeSettingsWindow();
  settingsSubRoute.set(DEFAULT_SETTINGS_SUB_ROUTE);
  settingsDirtyStore.set({});
  currentRoute.set("dashboard");
}

beforeEach(reset);
afterEach(reset);

describe("settings window (criterion 1)", () => {
  it("opens above the Chat and gives the Chat back when closed", () => {
    // GIVEN the Chat on screen
    navigateTo("chat");
    expect(get(currentRoute)).toBe("chat");

    // WHEN the settings route is navigated to
    navigateTo("settings");

    // THEN the window is open on a section, and the screen underneath is still the Chat
    expect(get(settingsWindow)).toEqual({ open: true, section: "general" });
    expect(get(currentRoute)).toBe("chat");

    // WHEN the window is closed (the cross and Escape call this request)
    const closed = requestCloseSettingsWindow();

    // THEN it is closed and the Chat is still the screen
    expect(closed).toBe(true);
    expect(get(settingsWindowOpen)).toBe(false);
    expect(get(currentRoute)).toBe("chat");
  });

  it("opens the window for any path that sets the settings route", () => {
    // GIVEN the Chat on screen
    navigateTo("chat");

    // WHEN the route store itself is set to settings (legacy callers do so)
    currentRoute.set("settings");

    // THEN the window opens and the screen does not change
    expect(get(settingsWindowOpen)).toBe(true);
    expect(get(currentRoute)).toBe("chat");
  });

  it("keeps the Chat mounted: the window sits outside the screen switch", () => {
    // GIVEN the shell that renders the screens
    const main = source("lib/components/app/Main.svelte");

    // WHEN its screen switch and the window are located
    const switchEnd = main.indexOf("{/key}");
    const window = main.indexOf("<SettingsWindow />");

    // THEN settings has no screen branch, and the window comes after the keyed switch
    expect(main).not.toContain('$currentRoute === "settings"');
    expect(switchEnd).toBeGreaterThan(0);
    expect(window).toBeGreaterThan(switchEnd);
  });

  it("closes by the cross and by Escape, both through the guarded request", () => {
    // GIVEN the window component and the settings header
    const win = source("components/settings/SettingsWindow.svelte");
    const settings = source("routes/Settings.svelte");

    // WHEN their close paths are read
    // THEN the dialog (Escape, backdrop) and the cross both call the guarded request
    expect(win).toContain("onclose={close}");
    expect(win).toContain("<Settings onclose={close} />");
    expect(win).toMatch(/function close\(\): void \{\s*requestCloseSettingsWindow\(\);/);
    expect(settings).toContain('data-testid="settings-window-close"');
    expect(source("lib/components/ui/dialog/Dialog.svelte")).toMatch(
      /event\.key === "Escape"\) \{\s*onclose\(\);/,
    );
  });

  it("closes the window when another screen is navigated to", () => {
    // GIVEN the window open above the dashboard
    navigateTo("settings");

    // WHEN the Chat is navigated to (a sidebar entry, a help topic)
    navigateTo("chat");

    // THEN the window is closed and the Chat is on screen
    expect(get(settingsWindowOpen)).toBe(false);
    expect(get(currentRoute)).toBe("chat");
  });
});

describe("System shortcuts (criterion 2)", () => {
  it("lists Cmd+Shift+R and Cmd+Shift+H in the catalogue, taken by no other entry", () => {
    // GIVEN the shortcut catalogue
    const engines = SHORTCUTS.find((s) => s.id === "settings.engines");
    const hardware = SHORTCUTS.find((s) => s.id === "settings.hardware");

    // WHEN the two entries and every other chord are read
    const others = SHORTCUTS.filter((s) => s !== engines && s !== hardware);

    // THEN both are catalogued, Ctrl+Shift outside macOS, and no other entry uses them
    expect(engines?.combo).toEqual({ mac: "⌘+⇧+R", win: "Ctrl+Shift+R" });
    expect(hardware?.combo).toEqual({ mac: "⌘+⇧+H", win: "Ctrl+Shift+H" });
    for (const s of others) {
      expect([s.combo.mac, s.combo.win]).not.toContain("⌘+⇧+R");
      expect([s.combo.mac, s.combo.win]).not.toContain("⌘+⇧+H");
      expect([s.combo.mac, s.combo.win]).not.toContain("Ctrl+Shift+R");
      expect([s.combo.mac, s.combo.win]).not.toContain("Ctrl+Shift+H");
    }
  });

  it("opens Engines and Hardware from the keyboard", () => {
    // GIVEN the global handlers registered, on a platform read as not macOS
    const dispose = registerGlobalShortcutHandlers();
    const mac = comboFor(SHORTCUTS[0]) === SHORTCUTS[0].combo.mac;
    const mods = mac ? { meta: true, shift: true } : { ctrl: true, shift: true };

    try {
      // WHEN the Engines chord is pressed
      dispatchShortcutEvent(chord("R", mods));

      // THEN the window is open on System > Engines
      expect(get(settingsWindow)).toEqual({ open: true, section: "engines" });

      // WHEN the Hardware chord is pressed
      dispatchShortcutEvent(chord("H", mods));

      // THEN the window shows System > Hardware
      expect(get(settingsWindow)).toEqual({ open: true, section: "hardware" });
    } finally {
      dispose();
    }
  });

  it("shows each chord beside its section in the column", () => {
    // GIVEN the System group of the column
    const system = SETTINGS_NAV.find((g) => g.labelKey === "settings.nav.cluster_system");

    // WHEN its entries are read
    const shortcuts = system?.entries.map((e) => [e.key, e.shortcutId]);

    // THEN each names the catalogue entry the column renders as a keycap
    expect(shortcuts).toEqual([
      ["engines", "settings.engines"],
      ["hardware", "settings.hardware"],
    ]);
    expect(Object.values(SETTINGS_SECTION_SHORTCUTS)).toEqual(["engines", "hardware"]);
    expect(source("routes/Settings.svelte")).toContain(
      'data-testid="settings-nav-{entry.key}-shortcut"',
    );
  });

  it("leaves another handler alone when the chord does not match", () => {
    // GIVEN one handler on the palette chord
    let fired = false;
    const dispose = registerShortcutHandler({ id: "palette.open", handler: () => (fired = true) });

    try {
      // WHEN Shift+R is pressed without the platform modifier
      dispatchShortcutEvent(chord("R", { shift: true }));

      // THEN nothing opens
      expect(fired).toBe(false);
      expect(get(settingsWindowOpen)).toBe(false);
    } finally {
      dispose();
    }
  });
});

describe("unsaved changes in the window (criterion 3)", () => {
  it("holds the window open on typed changes and closes it once resolved", () => {
    // GIVEN the window open on General with a typed field not yet saved, and
    // the guard the settings install (asks when explicit changes wait)
    navigateTo("chat");
    navigateTo("settings");
    setRouteState("general", { dirty: true, autoSave: "explicit" });
    let asked: (() => void) | null | undefined;
    const uninstall = installSettingsCloseGuard((then) => {
      if (!holdsExplicitChanges(get(settingsSubRoute))) return true;
      asked = then;
      return false;
    });

    try {
      // WHEN the window is closed on the way to the dashboard
      navigateTo("dashboard");

      // THEN the dialog is asked, the window stays open, the Chat stays on screen
      expect(asked).toBeTypeOf("function");
      expect(get(settingsWindowOpen)).toBe(true);
      expect(get(currentRoute)).toBe("chat");

      // WHEN the user discards (the dialog closes the window, then goes on)
      setRouteState("general", { dirty: false });
      closeSettingsWindow();
      asked?.();

      // THEN the window is closed and the dashboard is on screen
      expect(get(settingsWindowOpen)).toBe(false);
      expect(get(currentRoute)).toBe("dashboard");
    } finally {
      uninstall();
    }
  });

  it("lets switches and lists close the window without asking", () => {
    // GIVEN a change applied on the fly (debounced mode)
    setRouteState("appearance", { dirty: true, autoSave: "debounced" });

    // WHEN the predicate of the guard reads it
    // THEN it does not hold the window
    expect(holdsExplicitChanges("appearance")).toBe(false);
  });

  it("keeps the badge, Cmd+S, the dialog and its three answers in the window", () => {
    // GIVEN the settings shell and its section frame
    const settings = source("routes/Settings.svelte");
    const sub = source("components/settings/SettingsSubPage.svelte");

    // WHEN their pieces are looked up
    // THEN the badge, Cmd+S scoped to the open window, the dialog with
    // Discard, Save, Stay, the close guard, and the Reset and Save footer remain
    expect(settings).toContain("<UnsavedChangesBadge />");
    expect(settings).toContain("scope: inSettingsScope");
    expect(source("lib/keyboard/scopedShortcuts.ts")).toContain("get(settingsWindowOpen)");
    expect(settings).toContain("<UnsavedChangesDialog");
    expect(settings).toMatch(/onDiscard=\{onDiscard\}[\s\S]*onSave=\{onSaveAndContinue\}[\s\S]*onStay=\{onStay\}/);
    expect(settings).toContain("installSettingsCloseGuard(");
    expect(settings).toContain("finishPendingClose();");
    expect(sub).toContain("settings-subpage-reset");
    expect(sub).toContain("settings-subpage-save");
  });
});

describe("help topics (criterion 4)", () => {
  it("lead model to the AI screen, dictation and profile to their sections", () => {
    // GIVEN the help topics
    // WHEN their targets are read
    // THEN they follow the moves
    expect(HELP_TOPIC_TARGETS.model).toEqual({ route: "llm" });
    expect(HELP_TOPIC_TARGETS.stt).toEqual({ settings: "stt" });
    expect(HELP_TOPIC_TARGETS.profile).toEqual({ settings: "profile" });
  });

  it("opening the model topic from the window shows the AI screen", () => {
    // GIVEN the window open on General, where the help block lives
    navigateTo("settings");

    // WHEN the model topic is followed
    openHelpTopic("model");

    // THEN the window closes on the AI screen
    expect(get(settingsWindowOpen)).toBe(false);
    expect(get(currentRoute)).toBe("llm");
  });

  it.each([
    ["stt", "stt"],
    ["profile", "profile"],
  ])("opening the %s topic shows Settings > %s", (topic, section) => {
    // GIVEN the window open on General
    navigateTo("settings");

    // WHEN the topic is followed
    openHelpTopic(topic);

    // THEN the window stays open on its section
    expect(get(settingsWindow)).toEqual({ open: true, section });
  });
});

describe("entries to the settings (criterion 5)", () => {
  it("name no removed section anywhere in the application", () => {
    // GIVEN every source of the interface
    const files = sourceFiles(SRC);

    // WHEN the sections passed to the settings entries are collected
    const named = files.flatMap((file) =>
      [
        ...readFileSync(file, "utf8").matchAll(
          /(?:navigateToSettings|goToSettingsSubRoute|requestSettingsSubRoute|openSettingsWindow)\("([^"]+)"/g,
        ),
      ].map((m) => [file.slice(SRC.length + 1), m[1]]),
    );

    // THEN each is a section of the column
    expect(named.length).toBeGreaterThan(10);
    const removed = named.filter(([, sub]) => !(SETTINGS_SUB_ROUTES as string[]).includes(sub));
    expect(removed).toEqual([]);
  });

  it("route the palette, user menu and Transcriptions to the sections", () => {
    // GIVEN the palette, the user menu and the Transcriptions screen
    const palette = source("lib/command-palette/paletteIndex.ts");
    const actions = source("lib/command-palette/actions.ts");
    const menu = source("lib/components/app/UserMenu.svelte");
    const transcriptions = source("routes/Transcriptions.svelte");

    // WHEN their settings entries are read
    // THEN the palette opens each section, shortcuts lead to Chat, help and
    // about to General, and Transcriptions to Dictation
    expect(palette).toContain("navigateToSettings(sub);");
    expect(actions).toContain('navigateToSettings("chat");');
    expect(actions).toContain('navigateToSettings("general");');
    expect(menu).toContain('navigateToSettings("chat");');
    expect(menu).toContain('navigateToSettings("general");');
    expect(transcriptions.match(/navigateToSettings\("stt"\)/g)).toHaveLength(3);
  });

  it("lead a danger zone action to General, focused on that action", () => {
    // GIVEN the palette action that clears the memories
    const actions = source("lib/command-palette/actions.ts");
    expect(actions).toContain('navigateToSettings("general", "clear-memories");');
    expect(actions).toContain('navigateToSettings("general", "clear-logs");');
    expect(actions).toContain('navigateToSettings("general", "factory-reset");');

    // WHEN it is run
    navigateToSettings("general", "clear-memories");

    // THEN General opens and the danger zone reads the action to focus, once
    expect(get(settingsWindow)).toEqual({ open: true, section: "general" });
    expect(consumeSettingsFocus()).toBe("clear-memories");
    expect(consumeSettingsFocus()).toBeNull();
    expect(source("components/settings/DangerZone.svelte")).toContain("consumeSettingsFocus()");
  });

  it("lead a former section name to the section that holds its content", () => {
    // GIVEN an old caller naming the danger section
    // WHEN it opens the settings
    navigateToSettings("danger");

    // THEN General is shown
    expect(get(settingsWindow)).toEqual({ open: true, section: "general" });
  });
});
