import { describe, expect, it } from "vitest";
import { hotkeyFromSearch } from "./overlayHotkey";

describe("hotkeyFromSearch", () => {
  it("reads the percent-encoded hotkey the window was opened with", () => {
    // GIVEN the address the Rust side builds for the default hotkey
    const search = "?hotkey=ctrl%2Bshift%2Bspace";

    // WHEN the page reads it
    const hotkey = hotkeyFromSearch(search);

    // THEN the pluses come back as separators, not as spaces
    expect(hotkey).toBe("ctrl+shift+space");
  });

  it("keeps the placeholder when the address carries nothing", () => {
    // GIVEN an address without the parameter, and one with it left empty.
    // This is the control: a reader returning "" would print " pour arrêter".
    // WHEN the page reads them
    // THEN both say "no label", so the placeholder stays
    expect(hotkeyFromSearch("")).toBeNull();
    expect(hotkeyFromSearch("?hotkey=")).toBeNull();
    expect(hotkeyFromSearch("?hotkey=%20")).toBeNull();
  });
});
