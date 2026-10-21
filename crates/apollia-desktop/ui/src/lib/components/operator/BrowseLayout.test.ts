import { describe, test, expect } from "vitest";
import { render } from "svelte/server";
import { createRawSnippet } from "svelte";

import BrowseLayout, { browseListClass } from "./BrowseLayout.svelte";

const snippet = (html: string) => createRawSnippet(() => ({ render: () => html }));

function shell(props: Record<string, unknown>): string {
  return render(BrowseLayout, {
    props: {
      detailOpen: false,
      header: snippet(`<h2 data-testid="t-title">Title</h2>`),
      children: snippet(`<ul data-testid="t-list"></ul>`),
      "data-testid": "t-root",
      listTestid: "t-column",
      offlineTestid: "t-offline",
      ...props,
    } as never,
  }).body;
}

describe("browseListClass", () => {
  test("spans the width while nothing is open", () => {
    // GIVEN no detail card open
    // WHEN the classes of the list column are computed
    const classes = browseListClass(false);
    // THEN the column fills the width with the wide gutter
    expect(classes).toContain("flex-1");
    expect(classes).toContain("px-6");
    expect(classes).not.toContain("w-[360px]");
  });

  test("narrows to a ruled column beside an open card", () => {
    // GIVEN a detail card open
    // WHEN the classes of the list column are computed
    const classes = browseListClass(true);
    // THEN the column keeps its narrow width and a rule on its right
    expect(classes).toContain("w-[360px]");
    expect(classes).toContain("shrink-0");
    expect(classes).toContain("border-r");
    expect(classes).not.toContain("flex-1");
  });
});

describe("BrowseLayout", () => {
  test("renders the header, the list and no banner when online", () => {
    // GIVEN a shell with no offline message
    // WHEN it is rendered
    const html = shell({});
    // THEN the header and the list are there, the banner is not
    expect(html).toContain('data-testid="t-root"');
    expect(html).toContain('data-testid="t-title"');
    expect(html).toContain('data-testid="t-column"');
    expect(html).toContain('data-testid="t-list"');
    expect(html).not.toContain('data-testid="t-offline"');
  });

  test("closes the header with the banner in the words given", () => {
    // GIVEN an offline message
    // WHEN the shell is rendered
    const html = shell({ offlineMessage: "No network" });
    // THEN the banner shows those words inside the header
    const header = html.slice(html.indexOf("<header"), html.indexOf("</header>"));
    expect(header).toContain('data-testid="t-offline"');
    expect(header).toContain("No network");
  });

  test("renders the detail beside a narrowed list", () => {
    // GIVEN a detail card open
    // WHEN the shell is rendered
    const html = shell({ detailOpen: true, detail: snippet(`<aside data-testid="t-detail"></aside>`) });
    // THEN the card follows the narrowed list column
    expect(html).toContain("w-[360px]");
    expect(html.indexOf('data-testid="t-detail"')).toBeGreaterThan(html.indexOf('data-testid="t-list"'));
  });
});
