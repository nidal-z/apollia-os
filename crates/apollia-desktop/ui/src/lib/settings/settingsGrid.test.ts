/**
 * The settings grid: two groups ranked by use, and the content each section
 * received from the sections that left the navigation column.
 *
 * Components cannot be mounted under this Vitest configuration (node
 * environment, no rendering library), so the placement of moved content is
 * read from the sources the bundle is built from.
 */
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { SETTINGS_NAV } from "$lib/settings/settingsNav";
import { SETTINGS_SUB_ROUTES } from "$lib/stores/settings";

const SRC = resolve(__dirname, "../..");

function source(rel: string): string {
  return readFileSync(resolve(SRC, rel), "utf8");
}

function catalogue(locale: "fr" | "en"): Record<string, unknown> {
  return JSON.parse(source(`lib/i18n/${locale}.json`)) as Record<string, unknown>;
}

function lookup(tree: Record<string, unknown>, key: string): unknown {
  return key
    .split(".")
    .reduce<unknown>(
      (node, part) => (node as Record<string, unknown> | undefined)?.[part],
      tree,
    );
}

/** Index of each needle in `text`, failing loudly on a missing one. */
function positions(text: string, needles: string[]): number[] {
  return needles.map((needle) => {
    const at = text.indexOf(needle);
    expect(at, `${needle} missing`).toBeGreaterThanOrEqual(0);
    return at;
  });
}

describe("settings navigation column", () => {
  it("has the Settings group then the System group, in this order", () => {
    // GIVEN the navigation definition
    // WHEN its groups and entries are listed
    const layout = SETTINGS_NAV.map((g) => [g.labelKey, g.entries.map((e) => e.key)]);

    // THEN Settings comes first, Developer closes its use-ranked part, the
    // sections leaving the settings follow, and System holds Engines and Hardware
    expect(layout).toEqual([
      [
        "settings.nav.cluster_settings",
        [
          "general",
          "appearance",
          "profile",
          "chat",
          "stt",
          "model-defaults",
          "developer",
          "configuration",
          "llm",
          "model-hub",
          "tools",
          "permissions",
        ],
      ],
      ["settings.nav.cluster_system", ["engines", "hardware"]],
    ]);
  });

  it("lists every sub-route once and no section that moved away", () => {
    // GIVEN the entries of the column
    const keys = SETTINGS_NAV.flatMap((g) => g.entries.map((e) => e.key));

    // WHEN they are compared to the store and to the former sections
    // THEN they are exactly the sub-routes, and none of the former ones
    expect([...keys].sort()).toEqual([...SETTINGS_SUB_ROUTES].sort());
    for (const gone of ["about", "help", "danger", "shortcuts", "observability", "system"]) {
      expect(keys as string[]).not.toContain(gone);
    }
  });

  it.each(["fr", "en"] as const)("names every group and entry in the %s catalogue", (locale) => {
    // GIVEN one catalogue
    const tree = catalogue(locale);

    // WHEN each label key of the column is resolved
    const labels = SETTINGS_NAV.flatMap((g) => [g.labelKey, ...g.entries.map((e) => e.labelKey)]);

    // THEN each resolves to a non-empty string
    for (const key of labels) {
      expect(typeof lookup(tree, key), `${key} in ${locale}`).toBe("string");
    }
  });
});

describe("General gathers the moved content", () => {
  it("mounts the update panel exactly once under routes/", () => {
    // GIVEN every Svelte source under routes/
    const routeFiles = import.meta.glob("/src/routes/**/*.svelte", {
      query: "?raw",
      import: "default",
      eager: true,
    }) as Record<string, string>;

    // WHEN the update panel tags are counted
    const mounts = Object.entries(routeFiles).flatMap(([file, text]) =>
      (text.match(/<UpdateCheckPanel\b/g) ?? []).map(() => file),
    );

    // THEN there is one, in General
    expect(mounts).toEqual(["/src/routes/settings/General.svelte"]);
  });

  it("orders the update block as the capture: version, then check and channel on one row", () => {
    // GIVEN the General source
    const text = source("routes/settings/General.svelte");

    // WHEN the update block's parts are located
    const at = positions(text, [
      'data-testid="general-updates"',
      'data-testid="general-version"',
      'data-testid="general-update-channel"',
      "<UpdateCheckPanel",
      'data-testid="update-channel-select"',
      'data-testid="language-select"',
    ]);

    // THEN the version comes first, then the channel row holding the check
    // button before the channel selector, all before the next block
    expect([...at].sort((a, b) => a - b)).toEqual(at);
    const row = text.slice(at[2], text.indexOf("</SettingsFieldRow>", at[2]));
    expect(row).toContain("<UpdateCheckPanel");
    expect(row).toContain('data-testid="update-channel-select"');
  });

  it("carries updates, language, application folder, report, help, danger zone, then About", () => {
    // GIVEN the General source
    const text = source("routes/settings/General.svelte");

    // WHEN its blocks are located
    const at = positions(text, [
      'data-testid="general-updates"',
      "<UpdateCheckPanel",
      'data-testid="language-select"',
      'data-testid="about-copy-data-dir"',
      'data-testid="about-link-report"',
      "<HelpBlock",
      "<DangerZone",
      "<AboutBlock",
    ]);

    // THEN they appear in this order
    expect([...at].sort((a, b) => a - b)).toEqual(at);
  });

  it("keeps in its About block what runs locally, the licence, the credits, the commitment, the links and the contact", () => {
    // GIVEN the About block source
    const text = source("components/settings/about/AboutBlock.svelte");

    // WHEN its elements are located
    // THEN each is present
    positions(text, [
      'data-testid="about-local"',
      '"inference", "transcription", "memory", "audit"',
      "<AboutEthos",
      'data-testid="about-license"',
      "<AboutCredits",
      '"about-link-docs"',
      '"about-link-github"',
      'data-testid="about-copy-contact"',
    ]);
    // and the diagnostic copies are no longer there
    expect(text).not.toContain("about-copy-diagnostics");
  });

  it("keeps a danger zone of factory reset, clear memories and clear logs, without the onboarding reset", () => {
    // GIVEN the danger zone source
    const text = source("components/settings/DangerZone.svelte");

    // WHEN its actions are read
    // THEN the three destructive actions stay and the onboarding reset left
    positions(text, ['id: "factory-reset"', 'id: "clear-memories"', 'id: "clear-logs"']);
    expect(text).not.toContain("reset-onboarding");
  });
});

describe("Appearance, Chat, Developer and Observability", () => {
  it("Appearance carries the theme and the onboarding reminders reset, not the language", () => {
    // GIVEN the Appearance source
    const text = source("routes/settings/Appearance.svelte");

    // WHEN its controls are located
    // THEN the theme and the reminders reset are there, the language and plan mode are not
    positions(text, ["THEME_OPTIONS", 'data-testid="reset-onboarding-btn"', "resetOnboarding()"]);
    expect(text).not.toContain("LANGUAGE_OPTIONS");
    expect(text).not.toContain("planModeDefault");
  });

  it("Chat carries the plan mode default and the shortcut catalogue", () => {
    // GIVEN the Chat settings source
    const text = source("routes/settings/ChatSettings.svelte");

    // WHEN its content is located
    // THEN the plan mode toggle still writes the setting, and the catalogue follows
    positions(text, ["planModeDefault.set(next)", 'data-testid="plan-mode-default-toggle"', "<ShortcutCatalog"]);
  });

  it("Developer no longer carries the configuration, which has its own section", () => {
    // GIVEN the Developer source
    const text = source("routes/settings/Developer.svelte");

    // WHEN the former read-only view is searched
    // THEN it is gone: the editor of the file lives in Settings > Configuration
    expect(text).not.toContain("ConfigurationReadOnly");
    expect(text).not.toContain("settings.nav.configuration");
  });

  it("the capture and retention policy lives in the Observability screen", () => {
    // GIVEN the Observability screen and the settings shell
    const screen = source("routes/Observability.svelte");
    const shell = source("routes/Settings.svelte");

    // WHEN the policy panel is looked for
    // THEN the screen mounts it as a developer tab and the settings no longer load it
    positions(screen, ['"capture",\n  ]);', "<CapturePolicyPanel"]);
    expect(shell).not.toContain("Observability.svelte");
  });
});
