/**
 * Tool catalog - single source of truth for tool metadata used in the
 * tool selection UI (Chat picker + ChatConfigPanel).
 *
 * Covers all 10 native tools and provides operator-friendly and
 * builder-friendly labels/descriptions for each.
 */

import {
  Terminal,
  FileText,
  FilePen,
  FilePenLine,
  FolderOpen,
  Search,
  FileSearch,
  Globe,
  Code,
  Brain,
  Folder,
  Zap,
  Wifi,
  BookOpen,
  Compass,
} from "lucide-svelte";
// NOSONAR typescript:S1874 - Svelte 4 ComponentType retained for lucide-svelte compat
import type { ComponentType } from "svelte"; // NOSONAR typescript:S1874

/**
 * Local alias for Svelte 4 `ComponentType`. Retained for lucide-svelte type
 * compatibility - Svelte 5's `Component` does not match lucide-svelte's
 * exported icon signatures. Centralizes suppression of the deprecation lint.
 */
// NOSONAR typescript:S1874
type SvelteComponentType = ComponentType; // NOSONAR typescript:S1874

// ─────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────

export type ToolGroupId = "files" | "execution" | "network" | "research" | "memory";

/** Metadata for a logical tool group - used in operator-mode UI. */
export interface ToolGroupMeta {
  id: ToolGroupId;
  /** Group icon (shown in the picker chip and config panel header). */
  icon: SvelteComponentType;
  /** i18n key for the operator-friendly group label. */
  labelOperatorKey: string;
  /** i18n key for the builder-friendly group label. */
  labelBuilderKey: string;
  /** i18n key for the operator-friendly one-liner (shown in config panel). */
  descOperatorKey: string;
  /** Ordered list of tool IDs that belong to this group. */
  tools: string[];
}

/** Full metadata for a single native tool. */
export interface ToolMeta {
  id: string;
  /** Lucide icon component for this tool. */
  icon: SvelteComponentType;
  /** Which group this tool belongs to. */
  group: ToolGroupId;
  /** i18n key for the operator-friendly display name. */
  labelOperatorKey: string;
  /** i18n key for the operator-friendly one-line description. */
  descOperatorKey: string;
  /** i18n key for the builder-mode technical description. */
  descBuilderKey: string;
  /** Whether this tool requires explicit HITL approval each invocation. */
  dangerous: boolean;
  /** Whether this tool is on by default in new libre chat sessions. */
  defaultEnabled: boolean;
}

// ─────────────────────────────────────────────
// Groups
// ─────────────────────────────────────────────

export const TOOL_GROUPS: ToolGroupMeta[] = [
  {
    id: "files",
    icon: Folder,
    labelOperatorKey: "tools.groups.files.label_operator",
    labelBuilderKey: "tools.groups.files.label_builder",
    descOperatorKey: "tools.groups.files.desc_operator",
    tools: ["file_read", "file_write", "file_edit", "file_list", "file_glob", "file_grep"],
  },
  {
    id: "execution",
    icon: Zap,
    labelOperatorKey: "tools.groups.execution.label_operator",
    labelBuilderKey: "tools.groups.execution.label_builder",
    descOperatorKey: "tools.groups.execution.desc_operator",
    tools: ["bash_executor", "python_executor"],
  },
  {
    id: "network",
    icon: Wifi,
    labelOperatorKey: "tools.groups.network.label_operator",
    labelBuilderKey: "tools.groups.network.label_builder",
    descOperatorKey: "tools.groups.network.desc_operator",
    tools: ["http_fetch"],
  },
  {
    id: "research",
    icon: Compass,
    labelOperatorKey: "tools.groups.research.label_operator",
    labelBuilderKey: "tools.groups.research.label_builder",
    descOperatorKey: "tools.groups.research.desc_operator",
    tools: ["web_search", "web_read"],
  },
  {
    id: "memory",
    icon: Brain,
    labelOperatorKey: "tools.groups.memory.label_operator",
    labelBuilderKey: "tools.groups.memory.label_builder",
    descOperatorKey: "tools.groups.memory.desc_operator",
    tools: ["memory_search"],
  },
];

// ─────────────────────────────────────────────
// Catalog
// ─────────────────────────────────────────────

export const TOOL_CATALOG: ToolMeta[] = [
  // ── Files ──────────────────────────────────
  {
    id: "file_read",
    icon: FileText,
    group: "files",
    labelOperatorKey: "tools.catalog.file_read.label_operator",
    descOperatorKey: "tools.catalog.file_read.desc_operator",
    descBuilderKey: "tools.catalog.file_read.desc_builder",
    dangerous: false,
    defaultEnabled: true,
  },
  {
    id: "file_write",
    icon: FilePen,
    group: "files",
    labelOperatorKey: "tools.catalog.file_write.label_operator",
    descOperatorKey: "tools.catalog.file_write.desc_operator",
    descBuilderKey: "tools.catalog.file_write.desc_builder",
    dangerous: false,
    defaultEnabled: true,
  },
  {
    id: "file_edit",
    icon: FilePenLine,
    group: "files",
    labelOperatorKey: "tools.catalog.file_edit.label_operator",
    descOperatorKey: "tools.catalog.file_edit.desc_operator",
    descBuilderKey: "tools.catalog.file_edit.desc_builder",
    dangerous: false,
    defaultEnabled: true,
  },
  {
    id: "file_list",
    icon: FolderOpen,
    group: "files",
    labelOperatorKey: "tools.catalog.file_list.label_operator",
    descOperatorKey: "tools.catalog.file_list.desc_operator",
    descBuilderKey: "tools.catalog.file_list.desc_builder",
    dangerous: false,
    defaultEnabled: true,
  },
  {
    id: "file_glob",
    icon: Search,
    group: "files",
    labelOperatorKey: "tools.catalog.file_glob.label_operator",
    descOperatorKey: "tools.catalog.file_glob.desc_operator",
    descBuilderKey: "tools.catalog.file_glob.desc_builder",
    dangerous: false,
    defaultEnabled: true,
  },
  {
    id: "file_grep",
    icon: FileSearch,
    group: "files",
    labelOperatorKey: "tools.catalog.file_grep.label_operator",
    descOperatorKey: "tools.catalog.file_grep.desc_operator",
    descBuilderKey: "tools.catalog.file_grep.desc_builder",
    dangerous: false,
    defaultEnabled: true,
  },
  // ── Execution ──────────────────────────────
  {
    id: "bash_executor",
    icon: Terminal,
    group: "execution",
    labelOperatorKey: "tools.catalog.bash_executor.label_operator",
    descOperatorKey: "tools.catalog.bash_executor.desc_operator",
    descBuilderKey: "tools.catalog.bash_executor.desc_builder",
    dangerous: true,
    defaultEnabled: true,
  },
  {
    id: "python_executor",
    icon: Code,
    group: "execution",
    labelOperatorKey: "tools.catalog.python_executor.label_operator",
    descOperatorKey: "tools.catalog.python_executor.desc_operator",
    descBuilderKey: "tools.catalog.python_executor.desc_builder",
    dangerous: true,
    defaultEnabled: true,
  },
  // ── Network ────────────────────────────────
  {
    id: "http_fetch",
    icon: Globe,
    group: "network",
    labelOperatorKey: "tools.catalog.http_fetch.label_operator",
    descOperatorKey: "tools.catalog.http_fetch.desc_operator",
    descBuilderKey: "tools.catalog.http_fetch.desc_builder",
    dangerous: false,
    defaultEnabled: false,
  },
  // ── Research (web tools) ─────────
  {
    id: "web_search",
    icon: Compass,
    group: "research",
    labelOperatorKey: "tools.catalog.web_search.label_operator",
    descOperatorKey: "tools.catalog.web_search.desc_operator",
    descBuilderKey: "tools.catalog.web_search.desc_builder",
    dangerous: false,
    defaultEnabled: false,
  },
  {
    id: "web_read",
    icon: BookOpen,
    group: "research",
    labelOperatorKey: "tools.catalog.web_read.label_operator",
    descOperatorKey: "tools.catalog.web_read.desc_operator",
    descBuilderKey: "tools.catalog.web_read.desc_builder",
    dangerous: false,
    defaultEnabled: false,
  },
  // ── Memory ─────────────────────────────────
  {
    id: "memory_search",
    icon: Brain,
    group: "memory",
    labelOperatorKey: "tools.catalog.memory_search.label_operator",
    descOperatorKey: "tools.catalog.memory_search.desc_operator",
    descBuilderKey: "tools.catalog.memory_search.desc_builder",
    dangerous: false,
    defaultEnabled: false,
  },
];

// ─────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────

/** Default tool IDs enabled for new libre chat sessions. */
export const DEFAULT_ENABLED_TOOLS = new Set<string>(
  TOOL_CATALOG.filter((t) => t.defaultEnabled).map((t) => t.id)
);

/** Lookup map: tool id → ToolMeta */
export const TOOL_BY_ID = new Map<string, ToolMeta>(
  TOOL_CATALOG.map((t) => [t.id, t])
);

/** Return the enabled state of all tools in a group. */
export function getGroupState(
  group: ToolGroupMeta,
  enabledTools: Set<string>
): "all" | "some" | "none" {
  const enabled = group.tools.filter((id) => enabledTools.has(id)).length;
  if (enabled === 0) return "none";
  if (enabled === group.tools.length) return "all";
  return "some";
}

/**
 * Toggle a group: if all tools are enabled, disable all; otherwise enable all.
 * Returns a new Set - does not mutate the input.
 */
export function toggleGroup(
  group: ToolGroupMeta,
  enabledTools: Set<string>
): Set<string> {
  const next = new Set(enabledTools);
  if (getGroupState(group, enabledTools) === "all") {
    group.tools.forEach((id) => next.delete(id));
  } else {
    group.tools.forEach((id) => next.add(id));
  }
  return next;
}
