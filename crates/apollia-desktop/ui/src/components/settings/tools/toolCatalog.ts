/**
 * Static presentation catalog for the native-tools settings page.
 *
 * The backend `ToolStatusDto` carries no family/category field, so the grouping
 * and the display metadata (icon, configurability, default config for a reset)
 * live here on the front. This mirrors the chat tool vocabulary: icons are
 * reused from `$lib/tools/tool-display` (`TOOL_ICONS`) so a tool looks the same
 * in the chat activity strip and in Settings.
 *
 * FLAG (Phase 0.5): `TOOL_CATEGORIES` is a front-only map keyed by tool name.
 * If the native-tool catalog grows or ships MCP tools in the same list, promote
 * a `category` field onto `ToolStatusDto` (backend) instead of maintaining this.
 */
import { Notebook, MessageCircleQuestion, Wrench } from "lucide-svelte";
import { TOOL_ICONS } from "$lib/tools/tool-display";

/** Icon component type, borrowed from the shared chat icon map. */
type ToolIcon = (typeof TOOL_ICONS)[string];

/** Icons for native tools the chat map does not cover (settings-only surface). */
const EXTRA_ICONS: Record<string, ToolIcon> = {
  notebook_read: Notebook,
  notebook_edit: Notebook,
  ask_user: MessageCircleQuestion,
};

/** Resolves the lucide icon for a native tool, reusing the chat icons first. */
export function toolIcon(name: string): ToolIcon {
  return TOOL_ICONS[name] ?? EXTRA_ICONS[name] ?? Wrench;
}

export interface ToolCategory {
  /** Stable id, used for keying and testids. */
  id: string;
  /** i18n key for the family label. */
  labelKey: string;
  /** Tool names in this family, in display order. */
  tools: string[];
}

/** Native-tool families, in display order (see FLAG above). */
export const TOOL_CATEGORIES: ToolCategory[] = [
  {
    id: "web",
    labelKey: "settings.tools_page.cat_web",
    tools: ["web_search", "web_read", "http_fetch"],
  },
  {
    id: "exec",
    labelKey: "settings.tools_page.cat_exec",
    tools: ["bash_executor", "python_executor"],
  },
  {
    id: "files",
    labelKey: "settings.tools_page.cat_files",
    tools: ["file_read", "file_write", "file_edit", "file_list", "file_glob", "file_grep"],
  },
  {
    id: "notebooks",
    labelKey: "settings.tools_page.cat_notebooks",
    tools: ["notebook_read", "notebook_edit"],
  },
  {
    id: "memory",
    labelKey: "settings.tools_page.cat_memory",
    tools: ["memory_search", "ask_user"],
  },
];

const CONFIGURABLE = new Set(["web_search", "web_read"]);

/** Whether a tool opens the configuration drawer. */
export function isConfigurable(name: string): boolean {
  return CONFIGURABLE.has(name);
}

/** i18n key for a tool's humanized title. */
export function toolTitleKey(name: string): string {
  return `settings.tools_page.${name}_title`;
}

/** i18n key for a tool's humanized description. */
export function toolDescKey(name: string): string {
  return `settings.tools_page.${name}_desc`;
}

/**
 * Default config payload for `web_search`. Single source of truth shared by the
 * drawer form seeding and the context-menu "reset config" action, so the two
 * never drift.
 */
export const WEB_SEARCH_DEFAULT_CONFIG = {
  backend: "auto",
  require_configured: false,
  brave: { timeout_secs: 15, max_results: 10 },
  duckduckgo: { timeout_secs: 15, max_response_kb: 1024 },
} as const;

/** Default config payload for `web_read`. */
export const WEB_READ_DEFAULT_CONFIG = {
  timeout_secs: 20,
  max_response_kb: 2048,
  ssrf_guard: true,
} as const;

/** Default config payload for a tool, or null when it exposes no config. */
export function defaultToolConfig(name: string): Record<string, unknown> | null {
  if (name === "web_search") {
    return JSON.parse(JSON.stringify(WEB_SEARCH_DEFAULT_CONFIG)) as Record<string, unknown>;
  }
  if (name === "web_read") {
    return JSON.parse(JSON.stringify(WEB_READ_DEFAULT_CONFIG)) as Record<string, unknown>;
  }
  return null;
}
