import { describe, test, expect } from "vitest";
import type { OrgPolicyBody } from "$lib/ipc/policy";
import { formatSignedAt, invalidReasonKey, policySections } from "./policyRules";

function policy(): OrgPolicyBody {
  return {
    schema_version: 1,
    signed_at: "2026-10-02T09:30:00Z",
    connections: { allow: [], deny: [], remote: null },
    families: { deny: [] },
    inference: { allowed_servers: [], remote_endpoints: null, require_model_hash: false },
    settings: {},
    packages: { require_signature: false, trusted_publishers: [] },
    journal: { required: false, retention_days: null, export: { detail: null, daily_summary_dir: null } },
  };
}

describe("the rules of a policy in plain words", () => {
  test("a policy that imposes nothing has no section", () => {
    // GIVEN an empty policy
    const empty = policy();
    // WHEN its sections are built
    const sections = policySections(empty);
    // THEN there is none
    expect(sections).toEqual([]);
  });

  test("each section touched holds one line per rule, in the screen order", () => {
    // GIVEN a policy touching connections, journal and inference
    const p = policy();
    p.journal.export.detail = "forbid";
    p.connections.allow = ["browser", "imap"];
    p.inference.remote_endpoints = "deny";
    // WHEN its sections are built
    const sections = policySections(p);
    // THEN the sections follow the screen order and say each rule
    expect(sections.map((s) => s.id)).toEqual(["connections", "inference", "journal"]);
    expect(sections[0].rules).toEqual([
      { id: "connections-allow", labelKey: "policy.rule.connections_allow", value: "browser, imap", code: true },
    ]);
    expect(sections[2].rules[0].valueKey).toBe("policy.value.never");
  });

  test("a fixed setting is shown with its key and value", () => {
    // GIVEN a policy fixing two settings
    const p = policy();
    p.settings = { "desktop.developer_mode": false, "api.port": 7771 };
    // WHEN its sections are built
    const [settings] = policySections(p);
    // THEN each setting is a line, its value written out
    expect(settings.id).toBe("settings");
    expect(settings.rules.map((r) => [r.label, r.value])).toEqual([
      ["desktop.developer_mode", "false"],
      ["api.port", "7771"],
    ]);
  });
});

describe("the reason and the date", () => {
  test("each reason class has its key", () => {
    // GIVEN a refusal class
    // WHEN its sentence key is asked
    // THEN it is under policy.reason
    expect(invalidReasonKey("owner")).toBe("policy.reason.owner");
  });

  test("the signing date is the day written in the file", () => {
    // GIVEN a date signed late in the evening west of Greenwich
    const signedAt = "2026-10-02T23:30:00-05:00";
    // WHEN it is formatted in French and in English
    // THEN the day stays the one the organisation wrote
    expect(formatSignedAt(signedAt, "fr")).toBe("2 octobre 2026");
    expect(formatSignedAt(signedAt, "en")).toBe("October 2, 2026");
    expect(formatSignedAt("not a date", "fr")).toBe("not a date");
  });
});
