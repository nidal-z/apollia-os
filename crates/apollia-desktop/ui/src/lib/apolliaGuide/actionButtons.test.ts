import { describe, expect, it } from "vitest";
import {
  sanitizeActionButtons,
  validateActionButton,
  type RawActionButton,
} from "./actionButtons";

describe("apolliaGuide/actionButtons", () => {
  it("accepts allowlisted navigate routes", () => {
    const raw: RawActionButton = {
      label: "Ouvrir l'assistant",
      action: "navigate",
      payload: { route: "/automations?wizard=open" },
    };
    const safe = validateActionButton(raw);
    expect(safe).not.toBeNull();
    expect(safe!.target).toBe("automations");
    expect(safe!.query).toBe("wizard=open");
  });

  it("rejects unknown routes", () => {
    const raw: RawActionButton = {
      label: "x",
      action: "navigate",
      payload: { route: "/admin/destroy" },
    };
    expect(validateActionButton(raw)).toBeNull();
  });

  it("rejects unknown invoke commands", () => {
    const raw: RawActionButton = {
      label: "delete",
      action: "invoke",
      payload: { command: "delete_everything" },
    };
    expect(validateActionButton(raw)).toBeNull();
  });

  it("rejects buttons with empty labels", () => {
    const raw: RawActionButton = {
      label: "   ",
      action: "navigate",
      payload: { route: "/inbox" },
    };
    expect(validateActionButton(raw)).toBeNull();
  });

  it("sanitizeActionButtons caps at 3", () => {
    const raw: RawActionButton[] = Array.from({ length: 5 }, (_, i) => ({
      label: `btn ${i}`,
      action: "navigate" as const,
      payload: { route: "/inbox" },
    }));
    expect(sanitizeActionButtons(raw)).toHaveLength(3);
  });

  it("sanitizeActionButtons drops invalid items silently", () => {
    const raw: RawActionButton[] = [
      { label: "ok", action: "navigate", payload: { route: "/inbox" } },
      { label: "bad", action: "navigate", payload: { route: "/nope" } },
      { label: "ok2", action: "navigate", payload: { route: "/agents" } },
    ];
    const out = sanitizeActionButtons(raw);
    expect(out).toHaveLength(2);
    expect(out.map((b) => b.target)).toEqual(["inbox", "agents"]);
  });
});
