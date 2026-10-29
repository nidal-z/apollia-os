import { describe, it, expect, vi, beforeEach } from "vitest";
import { get } from "svelte/store";

const navigateTo = vi.hoisted(() => vi.fn());
vi.mock("$lib/stores/navigation", () => ({ navigateTo }));

import { connectionsView, requestConnection, INITIAL_CONNECTIONS_VIEW } from "./connectionsView";

describe("connectionsView", () => {
  beforeEach(() => {
    connectionsView.reset();
    navigateTo.mockClear();
  });

  it("keeps the card, the search, the chip and the state count across a new mount of the screen", () => {
    // GIVEN a card opened, words typed, a chip and a state count chosen
    connectionsView.select({ kind: "connection", id: "imap:ada@example.com" });
    connectionsView.search("fast");
    connectionsView.filter("imap");
    connectionsView.filterState("action");

    // WHEN the screen is mounted again and reads the store
    const view = get(connectionsView);

    // THEN it finds its place as it left it
    expect(view).toEqual({
      selection: { kind: "connection", id: "imap:ada@example.com" },
      query: "fast",
      kind: "imap",
      state: "action",
    });
  });

  it("closes the card without losing the search", () => {
    // GIVEN a card open over a search
    connectionsView.search("notion");
    connectionsView.select({ kind: "connection", id: "mcp:notion" });

    // WHEN the card is closed
    connectionsView.select(null);

    // THEN the search stays
    expect(get(connectionsView)).toEqual({ ...INITIAL_CONNECTIONS_VIEW, query: "notion" });
  });

  it("opens the page on a connection asked by another screen, with its row in the list", () => {
    // GIVEN the page left on a search, a chip and a state count that hide the connection
    connectionsView.search("drive");
    connectionsView.filter("folder");
    connectionsView.filterState("working");

    // WHEN another screen asks for the connection
    requestConnection("mcp:notion");

    // THEN the Connections screen opens on its card, search, chip and state count cleared
    expect(navigateTo).toHaveBeenCalledWith("integrations");
    expect(get(connectionsView)).toEqual({
      selection: { kind: "connection", id: "mcp:notion" },
      query: "",
      kind: "all",
      state: "all",
    });
  });
});
