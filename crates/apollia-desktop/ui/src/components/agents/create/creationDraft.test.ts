/**
 * The creation of an agent kept as a draft: what is worth keeping, what a
 * stored draft reads back as, and that it outlives the screen.
 */
import { beforeEach, describe, expect, test, vi } from "vitest";

const stored = new Map<string, string>();
vi.stubGlobal("localStorage", {
  getItem: (key: string) => stored.get(key) ?? null,
  setItem: (key: string, value: string) => void stored.set(key, value),
  removeItem: (key: string) => void stored.delete(key),
});
import {
  CREATION_STORAGE_KEY,
  currentCreation,
  draftStarted,
  endCreation,
  keepCreation,
  parseCreationDraft,
  type CreationDraft,
} from "./creationDraft";
import { emptyForm } from "./agentDraft";

function draft(over: Partial<CreationDraft> = {}): CreationDraft {
  return {
    phase: "form",
    wish: { ...emptyForm(), purpose: "Compare quotes" },
    runId: null,
    review: null,
    updatedAt: "2026-10-19T08:00:00.000Z",
    ...over,
  };
}

beforeEach(() => {
  endCreation("agents");
  endCreation("onboarding");
});

describe("the creation draft", () => {
  test("an empty form is not a draft, a word typed is", () => {
    // GIVEN an untouched form and one with its purpose typed
    // WHEN each is judged
    // THEN only the second is kept
    expect(draftStarted(draft({ wish: emptyForm() }))).toBe(false);
    expect(draftStarted(draft())).toBe(true);
    expect(draftStarted(draft({ wish: emptyForm(), runId: "run-1", phase: "drafting" }))).toBe(true);
  });

  test("a draft is kept in storage, apart for each place, and ends on demand", () => {
    // GIVEN a creation on the Agents screen with its workshop running
    const running = draft({ phase: "drafting", runId: "run-7" });

    // WHEN it is kept
    keepCreation("agents", running);

    // THEN it reads back as it was, from the store and from storage
    expect(currentCreation("agents")).toEqual(running);
    expect(parseCreationDraft(localStorage.getItem(`${CREATION_STORAGE_KEY}:agents`))).toEqual(running);
    // AND the first launch holds none
    expect(currentCreation("onboarding")).toBeNull();
    // AND ending it removes it from storage
    endCreation("agents");
    expect(localStorage.getItem(`${CREATION_STORAGE_KEY}:agents`)).toBeNull();
  });

  test("a stored draft that lost its run or its sheet opens on the form, and garbage reads as none", () => {
    // GIVEN a draft stored in the drafting phase without its run, one under
    // review without its sheet, and text that is not a draft
    const noRun = JSON.stringify(draft({ phase: "drafting", runId: null }));
    const noSheet = JSON.stringify(draft({ phase: "review", review: null }));

    // WHEN each is read back
    // THEN the first two open on the form, the last is none
    expect(parseCreationDraft(noRun)?.phase).toBe("form");
    expect(parseCreationDraft(noSheet)?.phase).toBe("form");
    expect(parseCreationDraft("{not json")).toBeNull();
    expect(parseCreationDraft(JSON.stringify({ phase: "form", wish: "x" }))).toBeNull();
  });
});
