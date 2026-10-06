/**
 * What the configuration editor reads in a text, and where it lives: the
 * comparison of two texts, the key of each line, the section of the developer
 * mode that holds the editor, and the read-only view it replaced.
 */
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  caretPosition,
  changedLines,
  keyLines,
  lineDiff,
  lineOffset,
  lockedLinesIntact,
} from "$lib/settings/configText";
import { visibleSettingsNav } from "$lib/settings/settingsNav";
import { settingsSubRouteVisible } from "$lib/stores/settings";

const SRC = resolve(__dirname, "../..");

function source(rel: string): string {
  return readFileSync(resolve(SRC, rel), "utf8");
}

describe("the comparison of two texts", () => {
  it("lists the removed then the added line of a changed value", () => {
    // GIVEN a value changed on line 2
    // WHEN the two texts are compared
    const diff = lineDiff("[api]\nport = 7771\n", "[api]\nport = 7772\n");

    // THEN line 2 is removed then added, the rest kept
    expect(diff.map((l) => `${l.kind}:${l.text}`)).toEqual([
      "same:[api]",
      "removed:port = 7771",
      "added:port = 7772",
      "same:",
    ]);
  });

  it("marks in the margin the lines of the buffer the file does not have", () => {
    // GIVEN a line changed and a line added
    // WHEN the margin lines are computed
    const lines = changedLines("a\nb\nc\n", "a\nB\nc\nd\n");

    // THEN lines 2 and 4 are marked
    expect([...lines]).toEqual([2, 4]);
  });
});

describe("the key of each line", () => {
  it("joins the table, dotted and quoted keys, and spans a value over lines", () => {
    // GIVEN tables, a dotted key, a quoted key, a comment and an array over lines
    const text = [
      "# comment",
      "[llm.observability]",
      "debug_log_prompt = true",
      '"quoted" = 1',
      "[tools]",
      "deny = [",
      '  "shell",',
      "]",
      "a.b = 2",
    ].join("\n");

    // WHEN each line is read
    const keys = keyLines(text);

    // THEN every assigning line has its full dotted key
    expect(Object.fromEntries(keys)).toEqual({
      3: "llm.observability.debug_log_prompt",
      4: "llm.observability.quoted",
      6: "tools.deny",
      7: "tools.deny",
      8: "tools.deny",
      9: "tools.a.b",
    });
  });

  it("keeps an edit that adds a line after a locked one", () => {
    // GIVEN a locked key
    const before = "[desktop]\ndeveloper_mode = true\n";

    // WHEN a blank line is added after it
    // THEN the locked lines are intact
    expect(lockedLinesIntact(before, `${before}\n`, ["desktop.developer_mode"])).toBe(true);
  });

  it("places the caret and the start of a line", () => {
    // GIVEN a three-line text
    const text = "ab\ncde\nf";

    // WHEN offsets and lines are converted
    // THEN line 2 starts at offset 3, and offset 5 is line 2 column 3
    expect(lineOffset(text, 2)).toBe(3);
    expect(caretPosition(text, 5)).toEqual({ line: 2, column: 3 });
  });
});

describe("Settings > Configuration", () => {
  it("is listed after Developer with the developer mode on, and absent with it off", () => {
    // GIVEN the settings column in both positions of the mode
    const keys = (on: boolean) => visibleSettingsNav(on).flatMap((g) => g.entries.map((e) => e.key));

    // WHEN the entries are listed
    const on = keys(true);
    const off = keys(false);

    // THEN Configuration follows Developer when on, and is gone when off
    expect(on.indexOf("configuration")).toBe(on.indexOf("developer") + 1);
    expect(off).not.toContain("configuration");
    expect(settingsSubRouteVisible("configuration", false)).toBe(false);
    expect(settingsSubRouteVisible("configuration", true)).toBe(true);
  });

  it("renders the editor only inside the developer-mode branch", () => {
    // GIVEN the section's source
    const page = source("routes/settings/Configuration.svelte");

    // WHEN the branch around the editor is read
    const open = page.indexOf("{#if $developerMode}");
    const editor = page.indexOf("<ConfigEditor />");
    const close = page.indexOf("{/if}");

    // THEN the editor sits inside it, so with the mode off it is not in the DOM
    expect(open).toBeGreaterThan(0);
    expect(editor).toBeGreaterThan(open);
    expect(close).toBeGreaterThan(editor);
  });

  it("numbers the lines and colors the text as TOML with the Shiki layer", () => {
    // GIVEN the editor and its code area
    const editor = source("components/settings/config/ConfigEditor.svelte");
    const area = source("components/settings/config/ConfigCodeArea.svelte");

    // WHEN their rendering is read
    // THEN the tokenizer is Shiki's for TOML, and the gutter prints each line number
    expect(editor).toContain('loadLineTokenizer("toml")');
    expect(editor).toContain("readConfigFile");
    expect(area).toContain('data-testid="config-editor-gutter-{line}"');
    expect(area).toMatch(/<span class="min-w-6 text-right tabular-nums">\{line\}<\/span>/);
  });

  it("disables Save from the state's verdict", () => {
    // GIVEN the editor's source
    const editor = source("components/settings/config/ConfigEditor.svelte");

    // WHEN the Save button is read
    // THEN its disabled state is the verdict of canSave
    expect(editor).toContain("const saveAllowed = $derived(canSave($editor));");
    expect(editor).toMatch(/disabled=\{!saveAllowed\}\s*data-testid="config-editor-save"/);
  });
});

describe("the read-only view is gone", () => {
  it("leaves no caller of the former commands, and keeps copy and reveal", () => {
    // GIVEN every source of the interface
    const files = import.meta.glob(["/src/**/*.svelte", "/src/**/*.ts", "!/src/**/*.test.ts"], {
      query: "?raw",
      import: "default",
      eager: true,
    }) as Record<string, string>;

    // WHEN the former names are searched
    const hits = Object.entries(files).filter(([, text]) =>
      /openConfigInEditor|open_config_in_editor|"get_config"|ConfigFileCard|ApollaConfigView/.test(text),
    );

    // THEN none is left, and the editor keeps the path's two actions
    expect(hits.map(([file]) => file)).toEqual([]);
    const editor = source("components/settings/config/ConfigEditor.svelte");
    expect(editor).toContain('data-testid="config-editor-copy-path"');
    expect(editor).toContain('data-testid="config-editor-reveal"');
  });
});
