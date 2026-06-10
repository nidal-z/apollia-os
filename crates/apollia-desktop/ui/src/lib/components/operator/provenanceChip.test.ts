import { describe, it, expect } from "vitest";
import { originToChip, hasReason } from "./provenanceChip";
import { PLAN_SESSION_KEYS } from "$lib/i18n/strings/planSession";

describe("originToChip", () => {
  it("maps initial to a neutral label and token", () => {
    // GIVEN an initial origin
    // WHEN computing the chip descriptor
    // THEN the label is origin_initial and the token is neutral
    const chip = originToChip("initial");
    expect(chip.labelKey).toBe(PLAN_SESSION_KEYS.originInitial);
    expect(chip.tokenClass).toContain("muted");
  });

  it("maps replan by interpolating the revision", () => {
    // GIVEN a replan origin at revision 2
    // WHEN computing the chip descriptor
    // THEN the label is origin_replan and labelValues carries revision = 2
    const chip = originToChip({ replan: 2 });
    expect(chip.labelKey).toBe(PLAN_SESSION_KEYS.originReplan);
    expect(chip.labelValues).toEqual({ revision: 2 });
  });

  it("maps user_inject and agent_edit to their own labels", () => {
    // GIVEN the injection and agent-edit origins
    // WHEN computing the chip descriptors
    // THEN each carries its own label key
    expect(originToChip("user_inject").labelKey).toBe(
      PLAN_SESSION_KEYS.originUserInject,
    );
    expect(originToChip("agent_edit").labelKey).toBe(
      PLAN_SESSION_KEYS.originAgentEdit,
    );
  });

  it("assigns a distinct token per origin", () => {
    // GIVEN the four origins
    // WHEN collecting their token classes
    // THEN the four tokens are distinct
    const classes = [
      originToChip("initial").tokenClass,
      originToChip({ replan: 1 }).tokenClass,
      originToChip("user_inject").tokenClass,
      originToChip("agent_edit").tokenClass,
    ];
    expect(new Set(classes).size).toBe(4);
  });
});

describe("hasReason", () => {
  it("is false for undefined, null or blank (error / partial case)", () => {
    // GIVEN absent or blank reasons
    // WHEN testing hasReason
    // THEN the result is false (reason section hidden)
    expect(hasReason(undefined)).toBe(false);
    expect(hasReason(null)).toBe(false);
    expect(hasReason("   ")).toBe(false);
  });

  it("is true for a non-blank reason", () => {
    // GIVEN a textual reason
    // WHEN testing hasReason
    // THEN the result is true
    expect(hasReason("duplicate step")).toBe(true);
  });
});
