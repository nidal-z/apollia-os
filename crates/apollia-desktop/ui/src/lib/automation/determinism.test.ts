/**
 * What a run resets before its first step: a creation of an agent left under
 * way by an earlier run is not reopened.
 */
import { describe, expect, test, vi } from "vitest";

const stored = new Map<string, string>();
vi.stubGlobal("localStorage", {
  getItem: (key: string) => stored.get(key) ?? null,
  setItem: (key: string, value: string) => void stored.set(key, value),
  removeItem: (key: string) => void stored.delete(key),
});
import { resetDeterministicUiState } from "./determinism";
import { CREATION_STORAGE_KEY, currentCreation, keepCreation } from "../../components/agents/create/creationDraft";
import { emptyForm } from "../../components/agents/create/agentDraft";

function leftover(phase: "form" | "drafting" | "review"): string {
  return JSON.stringify({
    phase,
    wish: { ...emptyForm(), purpose: "Summarise the press every morning." },
    runId: "run-create",
    review: null,
    updatedAt: "2026-10-30T08:00:00.000Z",
  });
}

describe("resetDeterministicUiState", () => {
  test("drops a creation an earlier run left in storage", () => {
    // GIVEN a creation under way, stored by an earlier run, for both scopes
    stored.set(`${CREATION_STORAGE_KEY}:agents`, leftover("drafting"));
    stored.set(`${CREATION_STORAGE_KEY}:onboarding`, leftover("form"));

    // WHEN the run resets the interface
    resetDeterministicUiState();

    // THEN neither scope reopens on it
    expect(currentCreation("agents")).toBeNull();
    expect(currentCreation("onboarding")).toBeNull();
    expect(stored.has(`${CREATION_STORAGE_KEY}:agents`)).toBe(false);
    expect(stored.has(`${CREATION_STORAGE_KEY}:onboarding`)).toBe(false);
  });

  test("drops a creation already held in memory", () => {
    // GIVEN a creation the Agents screen already read into its store
    keepCreation("agents", JSON.parse(leftover("drafting")));
    expect(currentCreation("agents")).not.toBeNull();

    // WHEN the run resets the interface
    resetDeterministicUiState();

    // THEN the store holds nothing
    expect(currentCreation("agents")).toBeNull();
  });
});
