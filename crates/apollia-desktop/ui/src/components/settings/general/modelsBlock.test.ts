import { describe, expect, it, vi } from "vitest";
import {
  MODELS_DIR_KEY,
  changeModelsDir,
  folderLocked,
  pickedFolder,
  tokenFieldState,
} from "./modelsBlock";

describe("tokenFieldState", () => {
  it("offers nothing but the field when no token is stored and nothing is typed", () => {
    // GIVEN no stored token and an empty field
    // WHEN the field state is computed
    const state = tokenFieldState("", "");
    // THEN nothing is pending, nothing can be saved or cleared
    expect(state).toEqual({ dirty: false, canSave: false, canClear: false });
  });

  it("marks a typed token as not yet saved and offers to save it", () => {
    // GIVEN no stored token
    // WHEN a token is typed, with blanks around it
    const state = tokenFieldState("  hf_abc ", "");
    // THEN the field is dirty and can be saved, and there is nothing to clear
    expect(state).toEqual({ dirty: true, canSave: true, canClear: false });
  });

  it("reads the stored token in the field as saved, and offers to clear it", () => {
    // GIVEN a stored token shown in the field
    // WHEN the field state is computed
    const state = tokenFieldState("hf_abc", "hf_abc");
    // THEN nothing is pending, and the token can be cleared
    expect(state).toEqual({ dirty: false, canSave: false, canClear: true });
  });

  it("does not offer to save an emptied field", () => {
    // GIVEN a stored token
    // WHEN the field is emptied
    const state = tokenFieldState("", "hf_abc");
    // THEN the change is pending but only clearing removes the token
    expect(state).toEqual({ dirty: true, canSave: false, canClear: true });
  });
});

describe("pickedFolder", () => {
  it("reads a path answer, an object answer, and a dismissal", () => {
    // GIVEN the three shapes a directory picker answers
    // WHEN each is read
    // THEN the path comes out, or null when the picker was dismissed
    expect(pickedFolder("/Volumes/Big/models")).toBe("/Volumes/Big/models");
    expect(pickedFolder({ path: "/data/models" })).toBe("/data/models");
    expect(pickedFolder(null)).toBeNull();
    expect(pickedFolder("")).toBeNull();
  });
});

describe("folderLocked", () => {
  it("locks the folder only when the policy imposes it", () => {
    // GIVEN the three sources of a setting
    // WHEN the lock is read for each
    // THEN only the policy locks
    expect(folderLocked("policy")).toBe(true);
    expect(folderLocked("user")).toBe(false);
    expect(folderLocked("default")).toBe(false);
  });
});

describe("changeModelsDir", () => {
  it("writes the models.dir setting, then answers the folder in force", async () => {
    // GIVEN a shell that accepts the write and resolves the new folder
    const writeSetting = vi.fn().mockResolvedValue({
      key: MODELS_DIR_KEY,
      value: "/Volumes/Big/models",
      source: "user",
    });
    const readFolder = vi.fn().mockResolvedValue("/Volumes/Big/models");

    // WHEN the folder is changed
    const folder = await changeModelsDir("/Volumes/Big/models", { writeSetting, readFolder });

    // THEN the key models.dir is written, and the resolved folder is answered
    expect(writeSetting).toHaveBeenCalledWith("models.dir", "/Volumes/Big/models");
    expect(readFolder).toHaveBeenCalledOnce();
    expect(folder).toBe("/Volumes/Big/models");
  });

  it("lets a refused write reach the caller", async () => {
    // GIVEN a shell that refuses the write (the policy imposes the folder)
    const writeSetting = vi.fn().mockRejectedValue("setting 'models.dir' is locked");
    const readFolder = vi.fn();

    // WHEN the folder is changed
    const change = changeModelsDir("/elsewhere", { writeSetting, readFolder });

    // THEN the refusal surfaces and the folder is not read again
    await expect(change).rejects.toBe("setting 'models.dir' is locked");
    expect(readFolder).not.toHaveBeenCalled();
  });
});
