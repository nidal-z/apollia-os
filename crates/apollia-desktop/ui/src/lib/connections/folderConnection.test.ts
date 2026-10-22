import { describe, test, expect, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null) }));

import { chooseAndAddFolder, chooseFolderAgain, folderAbilities, readablePath } from "./folderConnection";
import { REMOTE_DENIED_REASON } from "$lib/integrations/addConnection";
import type { Connection } from "$lib/types";

function folder(over: Partial<Connection> = {}): Connection {
  return {
    id: "folder:f1",
    kind: "folder",
    label: "Clients",
    account: null,
    path: "/Users/ada/Documents/Clients",
    provenance: "selection",
    remote: false,
    official: false,
    state: "connected",
    status: "working",
    reason: null,
    checkedAt: null,
    package: null,
    tools: [
      { id: "conn:folder:f1/file_read", family: "read", familySource: "builtin", prechecked: true },
      { id: "conn:folder:f1/file_write", family: "write_or_delete", familySource: "builtin", prechecked: false },
    ],
    ...over,
  };
}

describe("adding a folder", () => {
  test("the folder picked is added and answered as a connection", async () => {
    // GIVEN a picker answering a folder and a runtime that adds it
    const add = vi.fn(async () => folder());
    const pick = vi.fn(async () => "/Users/ada/Documents/Clients");

    // WHEN the folder is chosen
    const outcome = await chooseAndAddFolder("Choose", { pick, add });

    // THEN the picker opened with its title, the path went to the runtime,
    // and the connection is answered
    expect(pick).toHaveBeenCalledWith("Choose");
    expect(add).toHaveBeenCalledWith("/Users/ada/Documents/Clients");
    expect(outcome).toEqual({ kind: "added", connection: folder() });
  });

  test("a cancelled picker adds nothing", async () => {
    // GIVEN a picker the person closes
    const add = vi.fn(async () => folder());

    // WHEN the folder is chosen
    const outcome = await chooseAndAddFolder("Choose", { pick: async () => null, add });

    // THEN nothing reaches the runtime
    expect(outcome).toEqual({ kind: "cancelled" });
  });

  test("a refusal of the admission is told apart from a failure", async () => {
    // GIVEN a runtime refusing by the organisation policy, then one failing
    const pick = async () => "/Users/ada/Clients";
    const refusing = async () => {
      throw REMOTE_DENIED_REASON;
    };
    const failing = async () => {
      throw "the folder '/Users/ada/Clients' is already a connection";
    };

    // WHEN the folder is added through each
    const refused = await chooseAndAddFolder("Choose", { pick, add: refusing });
    const failed = await chooseAndAddFolder("Choose", { pick, add: failing });

    // THEN the first is a refusal by the organisation, the second a failure
    // carrying the runtime's words
    expect(refused).toMatchObject({ kind: "refused", refusal: { code: "remote_denied", by: "organisation" } });
    expect(failed).toEqual({
      kind: "failed",
      message: "the folder '/Users/ada/Clients' is already a connection",
    });
  });
});

describe("choosing the folder of a connection again", () => {
  test("the connection is pointed at the folder picked, then checked, under its identifier", async () => {
    // GIVEN a folder connection whose folder moved, a picker answering the new one
    const pick = vi.fn(async () => "/Users/ada/Archive/Clients");
    const repoint = vi.fn(async (id: string, path: string) => folder({ id, path }));
    const check = vi.fn(async (id: string) => folder({ id, path: "/Users/ada/Archive/Clients" }));

    // WHEN its folder is chosen again
    const outcome = await chooseFolderAgain("folder:f1", "Choose", { pick, repoint, check });

    // THEN the same connection is pointed at it, then checked
    expect(repoint).toHaveBeenCalledWith("folder:f1", "/Users/ada/Archive/Clients");
    expect(check).toHaveBeenCalledWith("folder:f1");
    expect(outcome).toEqual({ kind: "repaired", connection: folder({ path: "/Users/ada/Archive/Clients" }) });
  });

  test("a cancelled picker changes nothing, and a refusal is told apart", async () => {
    // GIVEN a picker that is cancelled, then a runtime that refuses the folder
    const repoint = vi.fn(async () => folder());
    const refusing = vi.fn(async () => {
      throw new Error(REMOTE_DENIED_REASON);
    });

    // WHEN the folder is chosen again each way
    const cancelled = await chooseFolderAgain("folder:f1", "Choose", { pick: async () => null, repoint });
    const refused = await chooseFolderAgain("folder:f1", "Choose", { pick: async () => "/x", repoint: refusing });

    // THEN the cancel reaches no command, and the refusal is a refusal
    expect(cancelled).toEqual({ kind: "cancelled" });
    expect(repoint).not.toHaveBeenCalled();
    expect(refused.kind).toBe("refused");
  });
});

describe("what a folder shows", () => {
  test("the abilities follow the families of its tools", () => {
    // GIVEN a folder whose tools read and write, and one whose tools only read
    const both = folder();
    const readOnly = folder({ tools: [both.tools[0]] });

    // WHEN its abilities are read
    // THEN reading and writing are each told only when a tool carries them
    expect(folderAbilities(both)).toEqual({ read: true, writeOrDelete: true });
    expect(folderAbilities(readOnly)).toEqual({ read: true, writeOrDelete: false });
  });

  test("the path reads as its last two folders", () => {
    // GIVEN paths of each shape
    // WHEN each is made readable
    // THEN the last two folders are joined by a chevron, a root stays as is
    expect(readablePath("/Users/ada/Documents/Clients")).toBe("Documents › Clients");
    expect(readablePath("C:\\Users\\ada\\Clients")).toBe("ada › Clients");
    expect(readablePath("/Clients")).toBe("Clients");
    expect(readablePath("/")).toBe("/");
  });
});
