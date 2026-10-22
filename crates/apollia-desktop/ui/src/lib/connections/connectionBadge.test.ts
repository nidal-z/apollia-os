import { describe, it, expect } from "vitest";
import { checkedDescriptor, stateBadge } from "./connectionBadge";

const NOW = Date.parse("2026-10-21T12:00:00Z");

describe("stateBadge", () => {
  it("gives each of the four states its word and its tone", () => {
    // GIVEN the four states the runtime finds
    // WHEN their badges are built
    const badges = (["working", "needs_repair", "paused", "blocked"] as const).map((s) =>
      stateBadge(s, null, null, NOW),
    );

    // THEN each has its own word and tone
    expect(badges.map((b) => b.labelKey)).toEqual([
      "connections.state.working",
      "connections.state.needs_repair",
      "connections.state.paused",
      "connections.state.blocked",
    ]);
    expect(badges.map((b) => b.variant)).toEqual(["success", "warning", "neutral", "danger"]);
  });

  it("says why a connection needs repair, and only then", () => {
    // GIVEN an account to sign in again, and a blocked connection with its reason
    // WHEN their badges are built
    const repair = stateBadge("needs_repair", { key: "sign_in_again" }, null, NOW);
    const blocked = stateBadge("blocked", { key: "forbidden_by_policy" }, null, NOW);

    // THEN the repair names its reason, the blocked one does not repeat it
    expect(repair.reasonKey).toBe("connections.state.reason.sign_in_again");
    expect(blocked.reasonKey).toBeNull();
  });

  it("dates the state found, and gives no date to one never checked", () => {
    // GIVEN a connection checked five minutes ago, and one never checked
    // WHEN their badges are built
    const checked = stateBadge("working", null, "2026-10-21T11:55:00Z", NOW);
    const never = stateBadge("not_checked", { key: "not_checked" }, null, NOW);

    // THEN the first says how long ago, the second says it was not checked, with no date
    expect(checked.when).toEqual({ key: "connections.state.checked_minutes_ago", values: { mins: 5 } });
    expect(never.when).toBeNull();
    expect(never.labelKey).toBe("connections.state.not_checked");
  });

  it("reads a connection never checked as neutral, undated, with Check, never as to repair", () => {
    // GIVEN a connection nothing has found in any state, even with a date and a reason given
    // WHEN its badge is built
    const never = stateBadge("not_checked", { key: "not_checked" }, "2026-10-21T11:55:00Z", NOW);

    // THEN its word is its own, its tone neutral, it carries no date and no
    // reason, and its one action is Check; a state found offers none here
    expect(never.labelKey).toBe("connections.state.not_checked");
    expect(never.labelKey).not.toBe("connections.state.needs_repair");
    expect(never.variant).toBe("neutral");
    expect(never.when).toBeNull();
    expect(never.reasonKey).toBeNull();
    expect(never.actionKey).toBe("connections.state.action.check");
    expect(stateBadge("working", null, null, NOW).actionKey).toBeNull();
  });
});

describe("checkedDescriptor", () => {
  it("counts in minutes, hours, then days", () => {
    // GIVEN dates at growing distances from now
    // WHEN each is described
    // THEN the unit grows with the distance
    expect(checkedDescriptor("2026-10-21T11:59:30Z", NOW)).toEqual({ key: "connections.state.checked_just_now" });
    expect(checkedDescriptor("2026-10-21T09:00:00Z", NOW)).toEqual({
      key: "connections.state.checked_hours_ago",
      values: { hours: 3 },
    });
    expect(checkedDescriptor("2026-10-18T12:00:00Z", NOW)).toEqual({
      key: "connections.state.checked_days_ago",
      values: { days: 3 },
    });
  });

  it("gives nothing for a date absent, unreadable or ahead of now", () => {
    // GIVEN no date, a malformed one and one in the future
    // WHEN each is described
    // THEN none is dated
    expect(checkedDescriptor(null, NOW)).toBeNull();
    expect(checkedDescriptor("yesterday", NOW)).toBeNull();
    expect(checkedDescriptor("2026-10-22T12:00:00Z", NOW)).toBeNull();
  });
});
