import { describe, test, expect, vi, beforeAll } from "vitest";
import { render as renderToString } from "svelte/server";
import { waitLocale } from "svelte-i18n";
import "$lib/i18n";

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

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn(async () => false),
  requestPermission: vi.fn(async () => "denied"),
  sendNotification: vi.fn(),
}));

import ChatConfigPanelBody from "./ChatConfigPanelBody.svelte";
import { developerMode } from "$lib/stores/developerMode";
import { isSensitiveFamily } from "$lib/tools/action-family";
import type { ChatSessionDetail } from "$lib/types";

function libreSession(): ChatSessionDetail {
  return {
    id: "s-1",
    mode: "libre",
    agent_name: null,
    system_prompt: "",
    status: "active",
    available_tools: [],
    authorized_tools: [],
    messages: [],
    created_at: "2026-10-01T00:00:00Z",
    closed_at: null,
    llm_backend: null,
    title: null,
    plan_mode: false,
    plan_phase: "discovery",
  };
}

/**
 * Renders the panel through Svelte's server renderer. Vitest runs this suite
 * in its node environment, where `svelte` resolves to the server build and
 * `mount` is unavailable; the markup is what this suite asserts on, and the
 * server render walks the same template branches.
 */
function render(): string {
  return renderToString(ChatConfigPanelBody, {
    props: { session: libreSession(), onupdated: () => {} },
  }).body;
}

/** The opening tag carrying `data-testid="<testid>"`, or `null`. */
function tagOf(html: string, testid: string): string | null {
  const match = html.match(new RegExp(`<[^>]*data-testid="${testid}"[^>]*>`));
  return match ? match[0] : null;
}

/** The `data-sensitive` value of a tool's icon, or `null` when absent. */
function sensitivityOf(html: string, toolId: string): string | null {
  const tag = tagOf(html, `config-tool-icon-${toolId}`);
  const match = tag?.match(/data-sensitive="([^"]*)"/);
  return match ? match[1] : null;
}

beforeAll(async () => {
  await waitLocale();
});

describe("isSensitiveFamily", () => {
  test("only reading and the exempt family are not sensitive", () => {
    // GIVEN every family, plus a value no build knows
    // WHEN each is classified
    // THEN reading and exempt are the only non-sensitive values
    expect(isSensitiveFamily("read")).toBe(false);
    expect(isSensitiveFamily("exempt")).toBe(false);
    for (const family of [
      "read_personal",
      "write_or_delete",
      "send_external",
      "execute_code",
      "unclassified",
      "teleport",
    ]) {
      expect(isSensitiveFamily(family)).toBe(true);
    }
  });
});

describe("ChatConfigPanelBody - tool sensitivity follows the action family", () => {
  test("a reading tool is plain, every other family is marked", () => {
    // GIVEN the builder view of a libre session's configuration
    developerMode.set(true);

    // WHEN the panel renders its tool list
    const html = render();
    const marker = (id: string) => tagOf(html, `config-tool-sensitive-${id}`);

    // THEN tools of the read family carry no warning
    for (const id of ["file_read", "file_list", "web_search"]) {
      expect(sensitivityOf(html, id), id).toBe("false");
      expect(marker(id), id).toBeNull();
    }
    // AND a tool of every other family is marked sensitive
    for (const id of [
      "memory_search",
      "file_write",
      "http_fetch",
      "bash_executor",
    ]) {
      expect(sensitivityOf(html, id), id).toBe("true");
      expect(marker(id), id).not.toBeNull();
    }
  });

  test("the operator view keeps the colour but drops the warning glyph", () => {
    // GIVEN the operator view
    developerMode.set(false);

    // WHEN the panel renders
    const html = render();

    // THEN a sensitive tool is still flagged on its icon, without the glyph
    expect(sensitivityOf(html, "file_write")).toBe("true");
    expect(tagOf(html, "config-tool-sensitive-file_write")).toBeNull();
  });
});
