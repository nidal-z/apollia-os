import { describe, expect, test } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { paneGrid } from "./chatPanes";
import {
  DRAG_THRESHOLD_PX,
  arrowTab,
  dragScroll,
  isDrag,
  paneColumnsClass,
  revealScroll,
  scrollRailClass,
  wheelDistance,
} from "./paneScroll";
import { chatPane } from "$lib/design/tokens";

const here = path.dirname(fileURLToPath(import.meta.url));
const appCss = readFileSync(path.resolve(here, "../../app.css"), "utf8");
const tailwindConfig = readFileSync(path.resolve(here, "../../../tailwind.config.ts"), "utf8");

describe("the scroll bar of a rail", () => {
  test("with the setting off, the rail scrolls and draws no bar", () => {
    // GIVEN the setting off
    // WHEN the classes of the rail are asked
    const classes = scrollRailClass(false).split(" ");

    // THEN it still scrolls sideways, and the bar is removed
    expect(classes).toContain("overflow-x-auto");
    expect(classes).toContain("scrollbar-none");
  });

  test("with the setting on, the bar is left to the overflow", () => {
    // GIVEN the setting on
    // WHEN the classes of the rail are asked
    const classes = scrollRailClass(true).split(" ");

    // THEN it scrolls sideways and nothing removes its bar
    expect(classes).toContain("overflow-x-auto");
    expect(classes).not.toContain("scrollbar-none");
  });

  test("a rail that holds its content draws no bar under either value", () => {
    // GIVEN both values of the setting
    // WHEN the classes of the rail are asked
    const both = [scrollRailClass(true), scrollRailClass(false)];

    // THEN neither forces a bar: the overflow is `auto`, never `scroll`
    for (const classes of both) {
      expect(classes).not.toMatch(/overflow-(x-)?scroll/);
    }
  });

  test("the class that removes the bar removes it in both engines", () => {
    // GIVEN the stylesheet of the application
    // WHEN the rule of `scrollbar-none` is read
    // THEN it covers the standard property and the WebKit pseudo-element
    expect(appCss).toMatch(/\.scrollbar-none\s*\{\s*scrollbar-width:\s*none;\s*\}/);
    expect(appCss).toMatch(/\.scrollbar-none::-webkit-scrollbar\s*\{\s*display:\s*none;\s*\}/);
  });
});

describe("scrolling the row of tabs", () => {
  test("a wheel that turns vertically moves the row sideways", () => {
    // GIVEN a mouse wheel, which reports a vertical distance only
    // WHEN its distance is read
    // THEN the row moves by that distance, in both directions
    expect(wheelDistance(0, 120)).toBe(120);
    expect(wheelDistance(0, -120)).toBe(-120);
  });

  test("a trackpad that swipes sideways keeps its axis", () => {
    // GIVEN a swipe that is mostly horizontal
    // WHEN its distance is read
    // THEN the horizontal part is used
    expect(wheelDistance(40, 6)).toBe(40);
  });

  test("a press that barely moves is a click, a longer one drags the row", () => {
    // GIVEN a row scrolled to 100
    // WHEN the pointer moves by the threshold, then beyond it to the left
    const still = isDrag(DRAG_THRESHOLD_PX);
    const moved = isDrag(-(DRAG_THRESHOLD_PX + 1));

    // THEN only the second is a drag, and the row follows the pointer
    expect(still).toBe(false);
    expect(moved).toBe(true);
    expect(dragScroll(100, -30)).toBe(130);
    expect(dragScroll(100, 30)).toBe(70);
  });

  test("a drag never scrolls before the start of the row", () => {
    // GIVEN a row scrolled to 10
    // WHEN it is dragged 50 to the right
    // THEN it stops at its start
    expect(dragScroll(10, 50)).toBe(0);
  });

  test("the arrow keys move from tab to tab and wrap at both ends", () => {
    // GIVEN four tabs
    const ids = ["main", "pane-2", "pane-3", "pane-4"];

    // WHEN an arrow, Home or End is pressed
    // THEN the neighbour is reached, and the row wraps
    expect(arrowTab(ids, "pane-2", "ArrowRight")).toBe("pane-3");
    expect(arrowTab(ids, "pane-2", "ArrowLeft")).toBe("main");
    expect(arrowTab(ids, "pane-4", "ArrowRight")).toBe("main");
    expect(arrowTab(ids, "main", "ArrowLeft")).toBe("pane-4");
    expect(arrowTab(ids, "pane-3", "Home")).toBe("main");
    expect(arrowTab(ids, "pane-2", "End")).toBe("pane-4");
  });

  test("another key is left alone", () => {
    // GIVEN two tabs
    // WHEN a key that is not an arrow is pressed
    // THEN no tab is reached
    expect(arrowTab(["main", "pane-2"], "main", "Enter")).toBeNull();
    expect(arrowTab([], "main", "ArrowRight")).toBeNull();
  });
});

describe("the tab of the active pane", () => {
  const rail = { scrollStart: 200, viewSize: 300 };

  test("a tab hidden on the right is brought to the right edge", () => {
    // GIVEN a row showing 200 to 500, and the active tab at 560 to 680
    // WHEN the row reveals it
    const next = revealScroll({ ...rail, itemStart: 560, itemSize: 120 });

    // THEN the row scrolls just enough for the tab to end at the edge
    expect(next).toBe(380);
  });

  test("a tab hidden on the left is brought to the left edge", () => {
    // GIVEN the same row, and the active tab at 40 to 160
    // WHEN the row reveals it
    const next = revealScroll({ ...rail, itemStart: 40, itemSize: 120 });

    // THEN the row scrolls back to the start of the tab
    expect(next).toBe(40);
  });

  test("a tab cut by an edge is brought whole into view", () => {
    // GIVEN the same row, and the active tab at 450 to 570, half hidden
    // WHEN the row reveals it
    const next = revealScroll({ ...rail, itemStart: 450, itemSize: 120 });

    // THEN the row shows it whole
    expect(next).toBe(270);
  });

  test("a tab already whole on screen does not move the row", () => {
    // GIVEN the same row, and the active tab at 250 to 370
    // WHEN the row reveals it
    const next = revealScroll({ ...rail, itemStart: 250, itemSize: 120 });

    // THEN the scroll position is unchanged
    expect(next).toBe(200);
  });

  test("a row that holds its tabs stays at its start", () => {
    // GIVEN a row of 300 that holds two tabs of 120, not scrolled
    // WHEN it reveals the second
    const next = revealScroll({ itemStart: 120, itemSize: 120, scrollStart: 0, viewSize: 300 });

    // THEN nothing scrolls
    expect(next).toBe(0);
  });
});

describe("panes side by side in a wide window", () => {
  test("two columns keep a minimal width per pane, read from the design scale", () => {
    // GIVEN two, three and four panes in a wide window
    // WHEN the column class of the area is asked
    const classes = [2, 3, 4].map((count) => paneColumnsClass(paneGrid(count, false)));

    // THEN each uses the columns whose floor is the pane token, which the
    // stylesheet defines and the typed tokens expose
    expect(classes).toEqual(["grid-cols-panes-2", "grid-cols-panes-2", "grid-cols-panes-2"]);
    expect(tailwindConfig).toContain(
      '"panes-2": "repeat(2, minmax(var(--chat-pane-min-width), 1fr))"',
    );
    expect(appCss).toMatch(/--chat-pane-min-width:\s*[\d.]+rem;/);
    expect(chatPane.minWidth).toBe("var(--chat-pane-min-width)");
  });

  test("one pane, or a narrow window, sets no floor", () => {
    // GIVEN a single pane, and four panes in a narrow window
    // WHEN the column class is asked
    // THEN the single column takes the width it is given
    expect(paneColumnsClass(paneGrid(1, false))).toBe("grid-cols-1");
    expect(paneColumnsClass(paneGrid(4, true))).toBe("grid-cols-1");
  });
});
