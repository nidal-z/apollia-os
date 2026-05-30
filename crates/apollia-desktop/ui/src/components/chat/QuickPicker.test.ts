import { describe, test, expect } from "vitest";
import type { AgentListItem } from "$lib/types";
import type { AgentLiveStatus } from "$lib/stores/agentStatus";
import { CHAT_TEMPLATES } from "$lib/templates/chatTemplates";

/**
 * Unit tests for the QuickPicker pure logic.
 *
 * The Svelte component itself is exercised through the dev app; these
 * tests cover the data transforms and helper derivations so regressions
 * surface without a full render environment.
 */

// ─── Helpers ─────────────────────────────────────────────────────────────────

function makeAgent(overrides: Partial<AgentListItem> = {}): AgentListItem {
  return {
    id: null,
    name: overrides.name ?? "alpha",
    version: "0.1.0",
    enabled: true,
    runtime_status: "active",
    installed_at: null,
    description: null,
    tags: [],
    tools_required: [],
    tools_optional: [],
    execution_mode: null,
    install_path: null,
    supports_a2a: false,
    skills: [],
    agent_type: null,
    examples: [],
    limitations: [],
    setup_notes: null,
    ...overrides,
  } as AgentListItem;
}

function statusOf(
  statuses: Record<string, AgentLiveStatus>,
  name: string,
): AgentLiveStatus {
  return statuses[name] ?? "offline";
}

// ─── Templates ───────────────────────────────────────────────────────────────

describe("chatTemplates", () => {
  test("seeds the 5 expected generic templates", () => {
    // GIVEN the hardcoded template list
    // WHEN we inspect it
    // THEN the 5 ultra-generic use-case templates are present
    const ids = CHAT_TEMPLATES.map((t) => t.id).sort((a, b) => a.localeCompare(b));
    expect(ids).toEqual(
      ["brainstorm", "draft-writing", "explain", "research", "summarize"].sort((a, b) =>
        a.localeCompare(b),
      ),
    );
  });

  test("every template exposes i18n keys for title, description and prompt", () => {
    for (const tpl of CHAT_TEMPLATES) {
      expect(tpl.titleKey.startsWith("chat.template.")).toBe(true);
      expect(tpl.descriptionKey.startsWith("chat.template.")).toBe(true);
      expect(tpl.promptKey.startsWith("chat.template.")).toBe(true);
    }
  });
});

// ─── Agent status mapping ────────────────────────────────────────────────────

describe("agent status derivation", () => {
  test("returns the mapped status when known", () => {
    // GIVEN a status map
    const statuses: Record<string, AgentLiveStatus> = { alpha: "online" };
    // WHEN the picker resolves the dot color
    // THEN it uses the mapped value
    expect(statusOf(statuses, "alpha")).toBe("online");
  });

  test("falls back to offline when the agent is missing", () => {
    expect(statusOf({}, "ghost")).toBe("offline");
  });
});

// ─── Agent filtering (visibleAgents logic) ───────────────────────────────────

describe("agent filtering", () => {
  test("hides system agents from the picker", () => {
    // GIVEN a mixed list of worker + system agents
    const list: AgentListItem[] = [
      makeAgent({ name: "alpha", agent_type: "worker" }),
      makeAgent({ name: "sys", agent_type: "system" }),
      makeAgent({ name: "beta", agent_type: null }),
    ];
    // WHEN we apply the QuickPicker filter
    const visible = list.filter((a) => a.agent_type !== "system");
    // THEN only non-system agents appear
    expect(visible.map((a) => a.name)).toEqual(["alpha", "beta"]);
  });

  test("detects empty state when list is empty", () => {
    const list: AgentListItem[] = []; // NOSONAR typescript:S4158
    expect(list.filter((a) => a.agent_type !== "system").length).toBe(0);
  });
});
