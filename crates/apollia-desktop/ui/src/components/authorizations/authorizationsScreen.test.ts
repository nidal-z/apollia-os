import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, test } from "vitest";

const here = dirname(fileURLToPath(import.meta.url));
const read = (relative: string): string => readFileSync(resolve(here, relative), "utf8");

describe("authorizations - where a fine rule can be written", () => {
  test("the dialog of a fine rule renders only in developer mode", () => {
    // GIVEN the source of the dialog
    const source = read("RuleDialog.svelte");

    // WHEN its markup is read
    const markup = source.slice(source.lastIndexOf("</script>"));

    // THEN the whole dialog sits under the developer mode
    expect(markup.trimStart().startsWith("</script>\n\n{#if $developerMode}")).toBe(true);
    expect(markup.trimEnd().endsWith("{/if}")).toBe(true);
  });

  test("the screen offers the creation only when the mode allows it", () => {
    // GIVEN the source of the screen
    const source = read("AuthorizationsScreen.svelte");

    // WHEN the add button and the function opening the dialog are read
    const button = source.indexOf('data-testid="authorizations-add-rule"');
    const guard = source.lastIndexOf("{#if canCreateRule($developerMode)}", button);
    const closing = source.indexOf("{/if}", guard);

    // THEN the button sits inside the guard, and opening the dialog checks the mode again
    expect(guard).toBeGreaterThan(-1);
    expect(closing).toBeGreaterThan(button);
    expect(source).toContain("if (!canCreateRule($developerMode)) return;");
  });

  test("the settings page no longer carries the rule form, the session, chat or audit sections", () => {
    // GIVEN the source of the settings page
    const source = read("../../routes/settings/Permissions.svelte");

    // WHEN its imports are read
    // THEN none of the former sections is mounted, and it leads to the screen
    for (const gone of ["AddRuleForm", "RulesSection", "SessionAuthSection", "ChatRulesSection", "AuditSection"]) {
      expect(source).not.toContain(gone);
    }
    expect(source).toContain('navigateTo("authorizations")');
  });
});

describe("agent sheet - the standing consents", () => {
  test("the sheet shows the agent's standing consents with the withdrawal of the screen", () => {
    // GIVEN the sources of the sheet and of its standing-consent section
    const sheet = read("../agents/sheet/AgentSheet.svelte");
    const section = read("../agents/sheet/SheetAlways.svelte");

    // WHEN what they mount and call is read
    // THEN the sheet mounts the section for its agent, and the section uses
    // the same line, the same withdrawal and the same undo as the screen
    expect(sheet).toContain("<SheetAlways {definitionId} />");
    expect(section).toContain('import AlwaysRow from "../../authorizations/AlwaysRow.svelte";');
    expect(section).toContain("await withdrawAlways(shellIpc, consent);");
    expect(section).toContain("await undoAlways(shellIpc, consent);");
    expect(section).toContain("alwaysOf(listed, id)");
  });
});
