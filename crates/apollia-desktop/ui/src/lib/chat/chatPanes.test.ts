import { beforeEach, describe, expect, test, vi } from "vitest";
import { get } from "svelte/store";

const ipc = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc }));

import { openModelLease, releaseModelLease } from "$lib/ipc/llmLoad";
import { MAIN_PANE, createLeaseKeeper } from "./modelLease";
import {
  MAX_PANES,
  activeConversation,
  activatePane,
  canSplit,
  closePane,
  createChatPanes,
  emptyPane,
  forgetConversation,
  isDivided,
  paneGrid,
  paneShown,
  showConversation,
  singlePane,
  splitPane,
  tabsShown,
  type PaneLayout,
} from "./chatPanes";

const QWEN = "/models/Qwen3-8B-Q4_K_M.gguf";

/** A layout of `sessions.length` panes, each on one conversation, the last active. */
function layoutOf(...sessions: string[]): PaneLayout {
  let layout = singlePane();
  sessions.forEach((sessionId, i) => {
    if (i > 0) layout = splitPane(layout);
    layout = showConversation(layout, sessionId);
  });
  return layout;
}

beforeEach(() => {
  ipc.mockReset();
  ipc.mockResolvedValue({});
});

describe("dividing the chat", () => {
  test("the chat opens on one empty pane, which is the active one", () => {
    // GIVEN nothing
    // WHEN the chat opens
    const layout = singlePane();

    // THEN one pane shows no conversation and holds the focus of the screen
    expect(layout.panes).toEqual([{ id: MAIN_PANE, sessionId: null }]);
    expect(layout.activeId).toBe(MAIN_PANE);
    expect(isDivided(layout)).toBe(false);
  });

  test("splitting adds an empty pane beside the active one and activates it", () => {
    // GIVEN one pane on a conversation
    const before = showConversation(singlePane(), "conv-1");

    // WHEN the user splits
    const after = splitPane(before);

    // THEN a second, empty pane sits after the first and is the active one
    expect(after.panes.map((p) => p.sessionId)).toEqual(["conv-1", null]);
    expect(after.activeId).toBe(after.panes[1]!.id);
    expect(after.activeId).not.toBe(MAIN_PANE);
    expect(isDivided(after)).toBe(true);
  });

  test("a fifth pane is refused: four conversations at most", () => {
    // GIVEN four panes
    const four = layoutOf("conv-1", "conv-2", "conv-3", "conv-4");
    expect(four.panes).toHaveLength(MAX_PANES);

    // WHEN the user splits once more
    const after = splitPane(four);

    // THEN nothing changes and the layout says no pane fits
    expect(after).toBe(four);
    expect(canSplit(four)).toBe(false);
    expect(canSplit(layoutOf("conv-1", "conv-2", "conv-3"))).toBe(true);
  });

  test("every pane has its own identifier, also after one was closed", () => {
    // GIVEN three panes, the middle one closed
    const three = layoutOf("conv-1", "conv-2", "conv-3");
    const two = closePane(three, three.panes[1]!.id);

    // WHEN the user splits again, twice
    const four = splitPane(splitPane(two));

    // THEN no two panes share an identifier
    const ids = four.panes.map((p) => p.id);
    expect(new Set(ids).size).toBe(4);
  });

  test("closing a pane removes it and its neighbour on the left takes over", () => {
    // GIVEN three panes, the last one active
    const three = layoutOf("conv-1", "conv-2", "conv-3");

    // WHEN the active pane closes
    const two = closePane(three, three.activeId);

    // THEN two panes remain and the one before it is active
    expect(two.panes.map((p) => p.sessionId)).toEqual(["conv-1", "conv-2"]);
    expect(activeConversation(two)).toBe("conv-2");
  });

  test("closing a pane that is not the active one keeps the active one", () => {
    // GIVEN three panes, the last one active
    const three = layoutOf("conv-1", "conv-2", "conv-3");

    // WHEN the first pane closes
    const two = closePane(three, MAIN_PANE);

    // THEN the active pane did not move
    expect(two.activeId).toBe(three.activeId);
    expect(activeConversation(two)).toBe("conv-3");
  });

  test("the last pane is emptied, never removed", () => {
    // GIVEN one pane on a conversation
    const one = showConversation(singlePane(), "conv-1");

    // WHEN it closes
    const after = closePane(one, MAIN_PANE);

    // THEN the pane stays, with no conversation
    expect(after.panes).toEqual([{ id: MAIN_PANE, sessionId: null }]);
  });

  test("a removed conversation closes the pane that showed it", () => {
    // GIVEN two panes
    const two = layoutOf("conv-1", "conv-2");

    // WHEN the first conversation is removed, then one no pane shows
    const one = forgetConversation(two, "conv-1");
    const same = forgetConversation(one, "conv-9");

    // THEN its pane is gone, and the other removal changes nothing
    expect(one.panes.map((p) => p.sessionId)).toEqual(["conv-2"]);
    expect(same).toBe(one);
  });
});

describe("the active pane", () => {
  test("a conversation picked in the list opens in the active pane", () => {
    // GIVEN two panes, the first made active again
    const two = activatePane(layoutOf("conv-1", "conv-2"), MAIN_PANE);

    // WHEN the user picks another conversation
    const after = showConversation(two, "conv-3");

    // THEN the active pane shows it and the other pane kept its own
    expect(after.panes.map((p) => p.sessionId)).toEqual(["conv-3", "conv-2"]);
    expect(activeConversation(after)).toBe("conv-3");
  });

  test("the right panel follows the pane that is activated", () => {
    // GIVEN two panes, the second active
    const two = layoutOf("conv-1", "conv-2");
    expect(activeConversation(two)).toBe("conv-2");

    // WHEN the first pane is activated
    const after = activatePane(two, MAIN_PANE);

    // THEN the conversation the panel reads is the one of that pane
    expect(activeConversation(after)).toBe("conv-1");
  });

  test("a conversation already on screen is not shown twice: its pane is activated", () => {
    // GIVEN two panes, the second active
    const two = layoutOf("conv-1", "conv-2");

    // WHEN the user picks the conversation of the first pane
    const after = showConversation(two, "conv-1");

    // THEN no pane changed conversation and the first one is active
    expect(after.panes.map((p) => p.sessionId)).toEqual(["conv-1", "conv-2"]);
    expect(after.activeId).toBe(MAIN_PANE);
  });

  test("a choice made in the selector of a pane opens in that pane", () => {
    // GIVEN two panes, the second active
    const two = layoutOf("conv-1", "conv-2");

    // WHEN the selector of the first pane opens a new conversation
    const after = showConversation(two, "conv-3", MAIN_PANE);

    // THEN the first pane shows it and becomes the active one
    expect(after.panes.map((p) => p.sessionId)).toEqual(["conv-3", "conv-2"]);
    expect(after.activeId).toBe(MAIN_PANE);
  });

  test("activating a pane that does not exist changes nothing", () => {
    // GIVEN two panes
    const two = layoutOf("conv-1", "conv-2");

    // WHEN an unknown pane is activated, or emptied
    // THEN the layout is the same object
    expect(activatePane(two, "pane-9")).toBe(two);
    expect(emptyPane(two, "pane-9")).toBe(two);
  });
});

describe("a narrow window", () => {
  test("one pane shows at a time, the others behind tabs", () => {
    // GIVEN three panes, the second active
    const three = layoutOf("conv-1", "conv-2", "conv-3");
    const layout = activatePane(three, three.panes[1]!.id);

    // WHEN the window is narrow
    const shown = layout.panes.filter((p) => paneShown(layout, p.id, true));

    // THEN the active pane alone is on screen, in one column, under the tabs
    expect(shown.map((p) => p.sessionId)).toEqual(["conv-2"]);
    expect(tabsShown(layout, true)).toBe(true);
    expect(paneGrid(layout.panes.length, true)).toEqual({ columns: 1, rows: 1, wide: null });
  });

  test("a tab activates its pane, which then shows alone", () => {
    // GIVEN three panes in a narrow window, the last active
    const three = layoutOf("conv-1", "conv-2", "conv-3");

    // WHEN the tab of the first pane is pressed
    const after = activatePane(three, MAIN_PANE);

    // THEN the first pane shows and the last one does not
    expect(paneShown(after, MAIN_PANE, true)).toBe(true);
    expect(paneShown(after, three.activeId, true)).toBe(false);
  });

  test("a single pane has no tabs, and a wide window has none either", () => {
    // GIVEN one pane, then two
    const one = layoutOf("conv-1");
    const two = layoutOf("conv-1", "conv-2");

    // WHEN the tabs are asked for
    // THEN they show for several panes in a narrow window only
    expect(tabsShown(one, true)).toBe(false);
    expect(tabsShown(two, false)).toBe(false);
    expect(tabsShown(two, true)).toBe(true);
  });

  test("a wide window shows every pane: two columns, then two rows of two", () => {
    // GIVEN one to four panes in a wide window
    const four = layoutOf("conv-1", "conv-2", "conv-3", "conv-4");

    // WHEN the grid is asked for
    // THEN every pane is on screen and the third of three takes its row
    expect(four.panes.every((p) => paneShown(four, p.id, false))).toBe(true);
    expect(paneGrid(1, false)).toEqual({ columns: 1, rows: 1, wide: null });
    expect(paneGrid(2, false)).toEqual({ columns: 2, rows: 1, wide: null });
    expect(paneGrid(3, false)).toEqual({ columns: 2, rows: 2, wide: 2 });
    expect(paneGrid(4, false)).toEqual({ columns: 2, rows: 2, wide: null });
  });
});

describe("the store of the panes", () => {
  test("it replaces the single selection: the layout is read from it", () => {
    // GIVEN the store of a chat
    const panes = createChatPanes(() => undefined);

    // WHEN a conversation opens, the screen is split and another one opens
    panes.show("conv-1");
    const split = panes.split();
    panes.show("conv-2");

    // THEN the store holds both, the second active
    const layout = get(panes);
    expect(split).toBe(true);
    expect(layout.panes.map((p) => p.sessionId)).toEqual(["conv-1", "conv-2"]);
    expect(activeConversation(layout)).toBe("conv-2");
  });

  test("splitting beyond four panes answers false and notifies nobody", () => {
    // GIVEN four panes and a subscriber
    const panes = createChatPanes(() => undefined);
    panes.split();
    panes.split();
    panes.split();
    const seen: number[] = [];
    const stop = panes.subscribe((layout) => seen.push(layout.panes.length));

    // WHEN the user splits again
    const split = panes.split();
    stop();

    // THEN the split is refused and the subscriber saw one value only
    expect(split).toBe(false);
    expect(seen).toEqual([4]);
  });

  test("a pane that closes or is emptied is released, one that changes conversation is not", () => {
    // GIVEN two panes on two conversations
    const released: string[] = [];
    const panes = createChatPanes((id) => released.push(id));
    panes.show("conv-1");
    panes.split();
    panes.show("conv-2");
    const second = get(panes).activeId;

    // WHEN the second pane changes conversation, then closes, then the first is emptied
    panes.show("conv-3");
    expect(released).toEqual([]);
    panes.close(second);
    panes.empty(MAIN_PANE);

    // THEN each pane was released once, when it stopped showing a conversation
    expect(released).toEqual([second, MAIN_PANE]);
  });

  test("closing an empty pane releases nothing", () => {
    // GIVEN two panes, the second one empty
    const released: string[] = [];
    const panes = createChatPanes((id) => released.push(id));
    panes.show("conv-1");
    panes.split();

    // WHEN the empty pane closes
    panes.close(get(panes).activeId);

    // THEN no lease was given back
    expect(released).toEqual([]);
    expect(get(panes).panes).toHaveLength(1);
  });
});

describe("the leases of the panes", () => {
  /** The commands sent, in order, with their arguments. */
  function sent(): unknown[][] {
    return ipc.mock.calls.map((call) => [call[0], call[1]]);
  }

  test("each pane holds the lease of its own conversation, also on a shared model", async () => {
    // GIVEN two panes, each on a conversation that talks to the same model
    const leases = createLeaseKeeper({ open: openModelLease, release: releaseModelLease });
    const panes = createChatPanes((id) => void leases.drop(id));
    panes.show("conv-1");
    panes.split();
    panes.show("conv-2");
    const second = get(panes).activeId;

    // WHEN each pane holds its lease
    await leases.hold(MAIN_PANE, "conv-1", QWEN);
    await leases.hold(second, "conv-2", QWEN);

    // THEN two leases are open on one model path, one per conversation
    expect(sent()).toEqual([
      ["open_model_lease", { conversationId: "conv-1", modelPath: QWEN }],
      ["open_model_lease", { conversationId: "conv-2", modelPath: QWEN }],
    ]);
    expect(leases.held(MAIN_PANE)?.modelPath).toBe(leases.held(second)?.modelPath);
  });

  test("closing a pane releases its lease and leaves the lease of the other", async () => {
    // GIVEN two panes holding a lease each
    const leases = createLeaseKeeper({ open: openModelLease, release: releaseModelLease });
    const panes = createChatPanes((id) => void leases.drop(id));
    panes.show("conv-1");
    panes.split();
    panes.show("conv-2");
    const second = get(panes).activeId;
    await leases.hold(MAIN_PANE, "conv-1", QWEN);
    await leases.hold(second, "conv-2", QWEN);
    ipc.mockClear();

    // WHEN the second pane closes
    panes.close(second);
    await leases.drop(second);

    // THEN its conversation alone gave its lease back
    expect(sent()).toEqual([["release_model_lease", { conversationId: "conv-2" }]]);
    expect(leases.held(second)).toBeNull();
    expect(leases.held(MAIN_PANE)).toEqual({ conversationId: "conv-1", modelPath: QWEN });
  });
});
