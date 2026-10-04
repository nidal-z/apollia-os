import { describe, expect, it } from "vitest";
import type { AgentPackageDetailView, AgentPackageListItem } from "$lib/types";
import {
  PROVIDED_ENTRY,
  emptyKinds,
  enumerate,
  filterEntries,
  installedOn,
  libraryEntries,
  packageDetail,
  providedDetail,
  signatureTally,
  whatGoes,
  whatStays,
  workflowTarget,
} from "./libraryState";

function listed(over: Partial<AgentPackageListItem> = {}): AgentPackageListItem {
  return {
    name: "crm",
    version: "1.0.0",
    description: "Leads and accounts.",
    agent_count: 1,
    agents: [],
    installed_at: "2026-10-01T08:00:00Z",
    root_path: "/data/agents/packages/crm",
    root_missing: false,
    signature: { publisher: "Acme", signed: true, trusted: true, key_fingerprint: "ab12" },
    embedded: false,
    counts: { agents: 1, workflows: 1, tools: 2, connections: 2 },
    ...over,
  };
}

function detailed(over: Partial<AgentPackageDetailView> = {}): AgentPackageDetailView {
  return {
    name: "crm",
    version: "1.0.0",
    description: "Leads and accounts.",
    author: "",
    agents: [],
    installed_at: "2026-10-01T08:00:00Z",
    updated_at: "2026-10-01T08:00:00Z",
    root_path: "/data/agents/packages/crm",
    root_missing: false,
    manifest: {},
    signature: { publisher: "Acme", signed: true, trusted: true, key_fingerprint: "ab12" },
    embedded: false,
    content: {
      agents: [{ id: "crm-agent", name: "Qualifier", description: "Qualify the requests." }],
      workflows: [{ id: "weekly", name: "weekly", description: "Weekly digest." }],
      tools: [{ name: "lookup", description: "Look up a lead.", family: "read" }],
      connections_brought: ["ledger"],
      connections_required: [{ type: "gmail", reason: "Read the requests" }],
    },
    automations: [{ id: "crm-daily", target: "crm-agent" }],
    ...over,
  };
}

describe("libraryEntries", () => {
  it("puts the packages provided with Apollia first, as one signed entry, then the others by name", () => {
    // GIVEN two provided packages and two installed ones, out of order
    const packages = [
      listed({ name: "veille", signature: { publisher: "", signed: false, trusted: false, key_fingerprint: null } }),
      listed({ name: "apollia-guide", embedded: true, version: "0.3.0", counts: { agents: 1, workflows: 0, tools: 0, connections: 0 } }),
      listed({ name: "crm" }),
      listed({ name: "onboarding-agent", embedded: true, version: "0.3.0", counts: { agents: 1, workflows: 0, tools: 1, connections: 0 } }),
    ];

    // WHEN the entries of the list are built
    const entries = libraryEntries(packages);

    // THEN the provided packages are one entry, counted together and signed
    expect(entries.map((e) => e.key)).toEqual([PROVIDED_ENTRY, "crm", "veille"]);
    expect(entries[0]).toMatchObject({
      provided: true,
      signed: true,
      version: "0.3.0",
      counts: { agents: 2, workflows: 0, tools: 1, connections: 0 },
      packages: ["apollia-guide", "onboarding-agent"],
    });
    expect(entries[1]).toMatchObject({ publisher: "Acme", signed: true, provided: false });
    expect(entries[2].signed).toBe(false);
  });

  it("shows no provided entry when Apollia ships no package", () => {
    // GIVEN only installed packages
    // WHEN the entries are built
    const entries = libraryEntries([listed()]);

    // THEN there is one entry, the package itself
    expect(entries.map((e) => e.key)).toEqual(["crm"]);
  });
});

describe("filterEntries and signatureTally", () => {
  const entries = libraryEntries([
    listed({ name: "apollia-guide", embedded: true }),
    listed({ name: "crm" }),
    listed({ name: "veille", signature: { publisher: "Ada", signed: false, trusted: false, key_fingerprint: null } }),
  ]);

  it("matches a package by name or publisher, and the provided entry by the name the screen gives it", () => {
    // GIVEN three entries
    // WHEN they are filtered by a publisher, a name, the provided label, and nothing
    const byPublisher = filterEntries(entries, " ada ", "Agents provided with Apollia");
    const byName = filterEntries(entries, "CRM", "Agents provided with Apollia");
    const provided = filterEntries(entries, "provided", "Agents provided with Apollia");
    const all = filterEntries(entries, "  ", "Agents provided with Apollia");

    // THEN each query keeps its own entry, and an empty one keeps them all
    expect(byPublisher.map((e) => e.key)).toEqual(["veille"]);
    expect(byName.map((e) => e.key)).toEqual(["crm"]);
    expect(provided.map((e) => e.key)).toEqual([PROVIDED_ENTRY]);
    expect(all).toHaveLength(3);
  });

  it("counts the entries, signed and not", () => {
    // GIVEN the provided entry, a signed package and an unsigned one
    // WHEN they are tallied
    const tally = signatureTally(entries);

    // THEN the provided entry counts as signed
    expect(tally).toEqual({ total: 3, signed: 2, unsigned: 1 });
  });
});

describe("an open package", () => {
  it("lists what goes by kind, automations included, and what stays", () => {
    // GIVEN an open package with one item of every kind and one automation
    const detail = packageDetail(detailed());

    // WHEN what its uninstallation removes and keeps is read
    const leaving = whatGoes(detail);
    const staying = whatStays(detail);

    // THEN each kind names its items, the brought connection goes and the
    // required one stays
    expect(leaving).toEqual([
      { kind: "agents", names: ["Qualifier"] },
      { kind: "workflows", names: ["weekly"] },
      { kind: "tools", names: ["lookup"] },
      { kind: "automations", names: ["crm-daily"] },
      { kind: "connections", names: ["ledger"] },
    ]);
    expect(staying).toEqual(["gmail"]);
  });

  it("leaves out of what goes the kinds the package does not hold", () => {
    // GIVEN a package of one agent, with no automation
    const detail = packageDetail(
      detailed({
        content: {
          agents: [{ id: "a", name: "A", description: "" }],
          workflows: [],
          tools: [],
          connections_brought: [],
          connections_required: [],
        },
        automations: [],
      }),
    );

    // WHEN what goes, what stays and what is missing are read
    // THEN only the agent goes, nothing stays, and three kinds are missing
    expect(whatGoes(detail)).toEqual([{ kind: "agents", names: ["A"] }]);
    expect(whatStays(detail)).toEqual([]);
    expect(emptyKinds(detail.content)).toEqual(["workflow", "tool", "connection"]);
  });

  it("names a workflow by its package outside the package", () => {
    // GIVEN the open package crm and the provided packages as one
    const crm = packageDetail(detailed());
    const provided = providedDetail([
      detailed({ name: "apollia-guide", embedded: true }),
      detailed({
        name: "onboarding-agent",
        embedded: true,
        installed_at: "2026-09-01T08:00:00Z",
        content: {
          agents: [{ id: "onboarding-agent", name: "Faire connaissance", description: "" }],
          workflows: [{ id: "welcome", name: "welcome", description: "" }],
          tools: [],
          connections_brought: [],
          connections_required: [],
        },
        automations: [],
      }),
    ]);

    // WHEN the target of a workflow is asked in each
    // THEN it carries the package that holds it, and the provided entry
    // merges the content and keeps the earliest installation date
    expect(workflowTarget(crm, "weekly")).toBe("crm/weekly");
    expect(workflowTarget(provided, "welcome")).toBe("onboarding-agent/welcome");
    expect(provided.content.agents.map((a) => a.name)).toEqual(["Qualifier", "Faire connaissance"]);
    expect(provided.installedAt).toBe("2026-09-01T08:00:00Z");
    expect(provided).toMatchObject({ key: PROVIDED_ENTRY, provided: true, signed: true });
  });
});

describe("enumerate and installedOn", () => {
  it("joins labels with commas and the last word before the final one", () => {
    // GIVEN three, two, one and no label
    // WHEN each is enumerated
    // THEN the last word comes once, before the final label
    expect(enumerate(["workflow", "outil", "connexion"], "ni")).toBe("workflow, outil ni connexion");
    expect(enumerate(["agent", "connexion"], "ni")).toBe("agent ni connexion");
    expect(enumerate(["outil"], "ni")).toBe("outil");
    expect(enumerate([], "ni")).toBe("");
  });

  it("formats a recorded date and answers nothing for what is not one", () => {
    // GIVEN a recorded date, an empty one and a text that is no date
    // WHEN each is formatted
    // THEN only the date reads, in the locale asked
    expect(installedOn("2026-10-01T08:00:00Z", "en-US")).toContain("2026");
    expect(installedOn("", "en-US")).toBe("");
    expect(installedOn("not a date", "en-US")).toBe("");
  });
});
