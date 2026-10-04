import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { get } from "svelte/store";
import { afterEach, describe, expect, test } from "vitest";
import { detailSidebar, shownTab } from "$lib/stores/detailSidebar";

const here = dirname(fileURLToPath(import.meta.url));
const read = (relative: string): string => readFileSync(resolve(here, relative), "utf8");

afterEach(() => detailSidebar.reset());

describe("journal screen - where the detail of an entry opens", () => {
  test("see more opens the entry tab of the detail sidebar, see less collapses it", () => {
    // GIVEN the detail sidebar collapsed
    expect(get(detailSidebar)).toEqual({ open: false, tab: "guide" });

    // WHEN the journal opens an entry, then leaves it
    detailSidebar.openEntry();
    const opened = get(detailSidebar);
    detailSidebar.closeEntry();
    const left = get(detailSidebar);

    // THEN the column showed the entry tab, then collapsed back on the guide
    expect(opened).toEqual({ open: true, tab: "entry" });
    expect(left).toEqual({ open: false, tab: "guide" });
  });

  test("leaving the journal does not collapse a guide the user opened", () => {
    // GIVEN the column open on the guide
    detailSidebar.openGuide();

    // WHEN the journal clears its entry
    detailSidebar.closeEntry();

    // THEN the guide stays open
    expect(get(detailSidebar)).toEqual({ open: true, tab: "guide" });
  });

  test("the entry tab is shown only while the journal offers an entry", () => {
    // GIVEN the column left on the entry tab
    // WHEN the tab to show is asked with and without an entry on offer
    // THEN it falls back on the guide once no entry is offered
    expect(shownTab("entry", true)).toBe("entry");
    expect(shownTab("entry", false)).toBe("guide");
    expect(shownTab("guide", true)).toBe("guide");
  });

  test("see more is offered to every user, outside any developer mode guard", () => {
    // GIVEN the source of a line of the journal
    const source = read("JournalRow.svelte");

    // WHEN the toggle is read
    const toggle = source.indexOf('data-testid="journal-entry-toggle"');

    // THEN the line carries it and never reads the developer mode
    expect(toggle).toBeGreaterThan(-1);
    expect(source).not.toContain("developerMode");
  });

  test("the screen unfolds in the list in developer mode and in the sidebar otherwise", () => {
    // GIVEN the source of the screen
    const source = read("JournalScreen.svelte");

    // WHEN its toggle and its row are read
    // THEN the expansion starts from the mode, the list unfolds only in developer
    // mode, and the reader's gesture opens the entry tab
    expect(source).toContain("expansion = expansionFor(developer);");
    expect(source).toContain("unfolded={$developerMode && isUnfolded(expansion, key)}");
    expect(source).toContain("detailSidebar.openEntry();");
    expect(source).toContain("void journal.ensureDetail(line);");
  });

  test("the technical details sit under the developer mode", () => {
    // GIVEN the source of the detail of an entry
    const source = read("JournalEntryDetail.svelte");

    // WHEN the technical block is read
    const block = source.indexOf('data-testid="journal-detail-technical"');
    const guard = source.lastIndexOf("{#if $developerMode && technical !== null}", block);

    // THEN it is inside the guard, while the parameters are outside it
    expect(guard).toBeGreaterThan(-1);
    expect(source.indexOf('data-testid="journal-detail-parameters"')).toBeLessThan(guard);
  });

  test("the screen filters no line itself: the filters go to the load", () => {
    // GIVEN the sources of the screen and of its live state
    const screen = read("JournalScreen.svelte");
    const state = read("useJournal.svelte.ts");

    // WHEN what they do with the filters is read
    // THEN a change of filter loads from the route, and nothing filters on a criterion
    expect(screen).toContain("void journal.load(asked, developer);");
    expect(state).toContain("fetchPage(ipc, filters, developerMode, new Date(), 0)");
    for (const source of [screen, state]) {
      expect(source).not.toMatch(/\.filter\([^)]*filters\./);
    }
  });
});

describe("journal screen - how it is reached", () => {
  test("the sidebar, the palette and the router each hold the journal", () => {
    // GIVEN the sources of the sidebar, of the palette and of the router
    const sidebar = read("../../lib/components/app/Sidebar.svelte");
    const palette = read("../../lib/command-palette/actions.ts");
    const main = read("../../lib/components/app/Main.svelte");

    // WHEN the journal is looked for in each
    // THEN the sidebar lists it among the primary entries, and both others lead to it
    expect(sidebar).toMatch(/route: "journal"\s+as Route, Icon: ScrollText,\s+labelKey: "nav\.sidebar\.journal"/);
    expect(palette).toContain('nav("nav.journal", "journal", "nav.journal", ScrollText');
    expect(main).toContain('{:else if shownRoute === "journal"}');
  });
});
