import { describe, expect, test } from "vitest";

import { currentFolder, goUp, openFolder } from "./onedriveTrail";

describe("onedriveTrail", () => {
  test("opening folders goes down one step each, back goes up, the root stays the root", () => {
    // GIVEN the root of the drive
    const root = openFolder([], { id: "c", name: "Clients" });

    // WHEN a folder is opened inside, then the user goes back twice
    const deeper = openFolder(root, { id: "a", name: "Ada" });
    const back = goUp(deeper);
    const stillRoot = goUp(goUp(back));

    // THEN the folder shown follows the steps, and the root is null
    expect(currentFolder(deeper)).toBe("a");
    expect(currentFolder(back)).toBe("c");
    expect(stillRoot).toEqual([]);
    expect(currentFolder(stillRoot)).toBeNull();
  });
});
