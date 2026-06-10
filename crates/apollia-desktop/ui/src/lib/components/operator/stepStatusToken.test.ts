import { describe, it, expect } from "vitest";
import { stepStatusToken } from "./stepStatusToken";

describe("stepStatusToken", () => {
  it("maps in_progress to primary with pulse", () => {
    // GIVEN status in_progress
    // WHEN mapped
    const tokens = stepStatusToken("in_progress");
    // THEN it uses the primary token and pulses
    expect(tokens.text).toBe("text-primary");
    expect(tokens.pulse).toBe(true);
  });

  it("maps completed to success without pulse", () => {
    // GIVEN status completed
    // WHEN mapped
    const tokens = stepStatusToken("completed");
    // THEN success token, no pulse
    expect(tokens.text).toBe("text-success");
    expect(tokens.pulse).toBe(false);
  });

  it("maps failed to destructive", () => {
    // GIVEN status failed
    // WHEN mapped
    // THEN destructive token
    expect(stepStatusToken("failed").text).toBe("text-destructive");
  });

  it("maps pending and skipped to muted (default branch)", () => {
    // GIVEN pending and skipped
    // WHEN mapped
    // THEN both resolve to the muted token with distinct labels
    expect(stepStatusToken("pending").text).toBe("text-muted-foreground");
    expect(stepStatusToken("pending").labelKey).toBe("plan_session.status_pending");
    expect(stepStatusToken("skipped").text).toBe("text-muted-foreground");
    expect(stepStatusToken("skipped").labelKey).toBe("plan_session.status_skipped");
  });

  it("never returns a hardcoded color value", () => {
    // GIVEN every status
    const all = ["pending", "in_progress", "completed", "skipped", "failed"] as const;
    // WHEN mapped
    // THEN no value contains a hex or hsl literal
    for (const s of all) {
      const tk = stepStatusToken(s);
      expect(tk.text).not.toMatch(/#|hsl\(/);
      expect(tk.border).not.toMatch(/#|hsl\(/);
      expect(tk.surface).not.toMatch(/#|hsl\(/);
    }
  });
});
