import { describe, test, expect, vi } from "vitest";

// Stores read `localStorage` at import time; the node environment has none.
vi.hoisted(() => {
  const memory = new Map<string, string>();
  globalThis.localStorage = {
    getItem: (k: string) => memory.get(k) ?? null,
    setItem: (k: string, v: string) => void memory.set(k, v),
    removeItem: (k: string) => void memory.delete(k),
    clear: () => memory.clear(),
    key: (i: number) => Array.from(memory.keys())[i] ?? null,
    get length() {
      return memory.size;
    },
  };
});

import { resolveInitialExpanded } from "./ReasoningCard.svelte";

describe("ReasoningCard - initial state", () => {
  test("with the setting on, a new block opens unfolded", () => {
    // GIVEN a thought block the user never touched, the setting on
    const input = {
      stored: null,
      defaultExpanded: undefined,
      expandByDefault: true,
      kind: "thinking" as const,
    };

    // WHEN the card resolves its first state
    const expanded = resolveInitialExpanded(input);

    // THEN it opens
    expect(expanded).toBe(true);
  });

  test("with the setting off, a new block stays folded", () => {
    // GIVEN the same block, the setting off
    const input = {
      stored: null,
      defaultExpanded: undefined,
      expandByDefault: false,
      kind: "thinking" as const,
    };

    // WHEN the card resolves its first state
    const expanded = resolveInitialExpanded(input);

    // THEN it stays closed
    expect(expanded).toBe(false);
  });

  test("the state the user chose on a block wins over the setting", () => {
    // GIVEN a block the user folded, the setting on
    const folded = { stored: "0", defaultExpanded: undefined, expandByDefault: true, kind: "thinking" as const };
    // AND a block the user unfolded, the setting off
    const unfolded = { stored: "1", defaultExpanded: undefined, expandByDefault: false, kind: "thinking" as const };

    // WHEN each card resolves its state
    const a = resolveInitialExpanded(folded);
    const b = resolveInitialExpanded(unfolded);

    // THEN each keeps the user's choice
    expect(a).toBe(false);
    expect(b).toBe(true);
  });

  test("a retry chain still opens with the setting off", () => {
    // GIVEN a retry block, the setting off
    const input = { stored: null, defaultExpanded: undefined, expandByDefault: false, kind: "retry" as const };

    // WHEN the card resolves its first state
    // THEN it opens, as before
    expect(resolveInitialExpanded(input)).toBe(true);
  });
});
