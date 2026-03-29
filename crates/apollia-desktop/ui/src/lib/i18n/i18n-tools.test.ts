import { describe, test, expect } from "vitest";
import en from "./en.json";
import fr from "./fr.json";

type JsonObject = Record<string, unknown>;

function getSection(json: JsonObject, ...path: string[]): JsonObject {
  let node: unknown = json;
  for (const key of path) {
    if (typeof node !== "object" || node === null) return {};
    node = (node as JsonObject)[key];
  }
  return typeof node === "object" && node !== null ? (node as JsonObject) : {};
}

const EXPECTED_LABELS = [
  "file_read",
  "file_write",
  "file_edit",
  "file_list",
  "file_glob",
  "file_grep",
  "bash_executor",
  "python_executor",
  "http_fetch",
  "memory_search",
  "mcp_generic",
];

const EXPECTED_DESCRIPTIONS = [
  "file_read",
  "file_read_partial",
  "file_write",
  "file_edit",
  "file_edit_replace_all",
  "file_list",
  "file_list_recursive",
  "file_glob",
  "file_glob_in",
  "file_grep",
  "file_grep_in",
  "bash_executor",
  "python_executor",
  "http_fetch_get",
  "http_fetch_post",
  "http_fetch_generic",
  "memory_search",
  "memory_search_ns",
  "mcp_generic",
];

const EXPECTED_OUTPUT_KEYS = [
  "file_read_summary",
  "file_read_truncated",
  "file_write_summary",
  "file_edit_summary",
  "file_list_summary",
  "file_glob_summary",
  "file_grep_summary",
  "file_grep_truncated",
  "http_fetch_summary",
  "memory_search_summary",
  "bash_executor_summary",
  "python_executor_summary",
];

const EXPECTED_OUTPUTS_KEYS = [
  "file_read",
  "file_write",
  "file_edit",
  "file_list",
  "file_glob",
  "file_grep",
  "http_fetch",
  "memory_search",
  "bash_executor",
  "python_executor",
];

const EXPECTED_STATUS_KEYS = ["pending", "authorized", "executed", "refused"];

// ─── EN completeness ──────────────────────────────────────────────────────────

describe("i18n tools — EN completeness", () => {
  test("tools.labels has all 11 native tool keys", () => {
    // GIVEN en.json loaded
    // WHEN checking tools.labels
    // THEN all 11 required keys are present
    const labels = getSection(en as unknown as JsonObject, "tools", "labels");
    for (const key of EXPECTED_LABELS) {
      expect(labels, `missing tools.labels.${key}`).toHaveProperty(key);
    }
    expect(Object.keys(labels).length).toBeGreaterThanOrEqual(11);
  });

  test("tools.descriptions has 19+ template keys", () => {
    // GIVEN en.json loaded
    // WHEN checking tools.descriptions
    // THEN at least 19 keys are present including all contextual variants
    const descriptions = getSection(en as unknown as JsonObject, "tools", "descriptions");
    for (const key of EXPECTED_DESCRIPTIONS) {
      expect(descriptions, `missing tools.descriptions.${key}`).toHaveProperty(key);
    }
    expect(Object.keys(descriptions).length).toBeGreaterThanOrEqual(19);
  });

  test("tools.output has 12 summary keys", () => {
    // GIVEN en.json loaded
    // WHEN checking tools.output
    // THEN all 12 output summary keys are present
    const output = getSection(en as unknown as JsonObject, "tools", "output");
    for (const key of EXPECTED_OUTPUT_KEYS) {
      expect(output, `missing tools.output.${key}`).toHaveProperty(key);
    }
  });

  test("tools.outputs has keys for all 10 native tools", () => {
    // GIVEN en.json loaded
    // WHEN checking tools.outputs (used by OperatorToolCard)
    // THEN a key is present for each native tool
    const outputs = getSection(en as unknown as JsonObject, "tools", "outputs");
    for (const key of EXPECTED_OUTPUTS_KEYS) {
      expect(outputs, `missing tools.outputs.${key}`).toHaveProperty(key);
    }
  });

  test("tools.status has the 4 status keys", () => {
    // GIVEN en.json loaded
    // WHEN checking tools.status
    // THEN pending, authorized, executed, refused are all present
    const status = getSection(en as unknown as JsonObject, "tools", "status");
    for (const key of EXPECTED_STATUS_KEYS) {
      expect(status, `missing tools.status.${key}`).toHaveProperty(key);
    }
    expect(Object.keys(status).length).toBe(4);
  });

  test("chat.authorize_action is present with expected value", () => {
    // GIVEN en.json loaded
    // WHEN checking chat.authorize_action
    // THEN the key exists with the correct value
    const chat = getSection(en as unknown as JsonObject, "chat");
    expect(chat).toHaveProperty("authorize_action");
    expect(chat.authorize_action).toBe("The assistant would like to perform an action");
  });
});

// ─── FR completeness ──────────────────────────────────────────────────────────

describe("i18n tools — FR completeness", () => {
  test("tools.labels has all 11 native tool keys", () => {
    const labels = getSection(fr as unknown as JsonObject, "tools", "labels");
    for (const key of EXPECTED_LABELS) {
      expect(labels, `missing FR tools.labels.${key}`).toHaveProperty(key);
    }
  });

  test("tools.descriptions has 19+ template keys", () => {
    const descriptions = getSection(fr as unknown as JsonObject, "tools", "descriptions");
    for (const key of EXPECTED_DESCRIPTIONS) {
      expect(descriptions, `missing FR tools.descriptions.${key}`).toHaveProperty(key);
    }
    expect(Object.keys(descriptions).length).toBeGreaterThanOrEqual(19);
  });

  test("tools.output has 12 summary keys", () => {
    const output = getSection(fr as unknown as JsonObject, "tools", "output");
    for (const key of EXPECTED_OUTPUT_KEYS) {
      expect(output, `missing FR tools.output.${key}`).toHaveProperty(key);
    }
  });

  test("tools.outputs has keys for all 10 native tools", () => {
    const outputs = getSection(fr as unknown as JsonObject, "tools", "outputs");
    for (const key of EXPECTED_OUTPUTS_KEYS) {
      expect(outputs, `missing FR tools.outputs.${key}`).toHaveProperty(key);
    }
  });

  test("tools.status has the 4 status keys", () => {
    const status = getSection(fr as unknown as JsonObject, "tools", "status");
    for (const key of EXPECTED_STATUS_KEYS) {
      expect(status, `missing FR tools.status.${key}`).toHaveProperty(key);
    }
  });

  test("chat.authorize_action is present", () => {
    const chat = getSection(fr as unknown as JsonObject, "chat");
    expect(chat).toHaveProperty("authorize_action");
    expect(typeof chat.authorize_action).toBe("string");
    expect((chat.authorize_action as string).length).toBeGreaterThan(0);
  });
});

// ─── EN / FR parity ───────────────────────────────────────────────────────────

describe("i18n tools — EN/FR key parity", () => {
  function collectLeaves(obj: JsonObject, prefix = ""): string[] {
    const keys: string[] = [];
    for (const [k, v] of Object.entries(obj)) {
      const full = prefix ? `${prefix}.${k}` : k;
      if (typeof v === "object" && v !== null && !Array.isArray(v)) {
        keys.push(...collectLeaves(v as JsonObject, full));
      } else {
        keys.push(full);
      }
    }
    return keys;
  }

  test("every tools.* key in EN has an equivalent in FR", () => {
    // GIVEN en.json and fr.json loaded
    // WHEN comparing all tools.* leaf keys
    // THEN every EN key is present in FR
    const enTools = getSection(en as unknown as JsonObject, "tools");
    const frTools = getSection(fr as unknown as JsonObject, "tools");
    const enKeys = collectLeaves(enTools);
    const frKeys = new Set(collectLeaves(frTools));
    for (const key of enKeys) {
      expect(frKeys, `FR is missing tools.${key}`).toContain(key);
    }
  });

  test("chat.authorize_action exists in both languages", () => {
    // GIVEN en.json and fr.json
    // WHEN checking chat.authorize_action
    // THEN both files have the key with a non-empty string
    const enChat = getSection(en as unknown as JsonObject, "chat");
    const frChat = getSection(fr as unknown as JsonObject, "chat");
    expect(enChat).toHaveProperty("authorize_action");
    expect(frChat).toHaveProperty("authorize_action");
    expect(typeof enChat.authorize_action).toBe("string");
    expect(typeof frChat.authorize_action).toBe("string");
  });
});
