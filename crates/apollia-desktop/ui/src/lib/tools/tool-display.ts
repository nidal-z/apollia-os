import type { ToolCallRationale, ToolCallView } from "$lib/types";
import {
  FileText,
  FilePen,
  FilePenLine,
  FolderOpen,
  Search,
  FileSearch,
  Terminal,
  Code,
  Globe,
  Brain,
  Plug,
  Compass,
  BookOpen,
} from "lucide-svelte";
import type { ComponentType } from "svelte";

/** Display metadata for a tool call in the chat UI. */
export interface ToolDisplayInfo {
  /** Lucide icon component for this tool. */
  icon: ComponentType;
  /** i18n key for the operator-friendly label. */
  labelKey: string;
  /** i18n key for the operator-friendly description template. */
  descriptionKey: string;
  /** Parameters extracted from tool input for template interpolation. */
  templateParams: Record<string, string>;
  /** i18n key for the operator-friendly output summary template, or null if not applicable. */
  outputSummaryKey: string | null;
  /** Parameters extracted from tool output for output summary template interpolation. */
  outputParams: Record<string, string>;
}

const TOOL_ICONS: Record<string, ComponentType> = {
  file_read: FileText,
  file_write: FilePen,
  file_edit: FilePenLine,
  file_list: FolderOpen,
  file_glob: Search,
  file_grep: FileSearch,
  bash_executor: Terminal,
  python_executor: Code,
  http_fetch: Globe,
  web_search: Compass,
  web_read: BookOpen,
  memory_search: Brain,
};

/**
 * Resolves the display metadata for a tool call.
 *
 * Covers all 10 native tools, MCP tools (mcp:server/tool pattern), and
 * falls back to a generic display for unknown tools.
 */
export function resolveToolDisplay(toolCall: ToolCallView): ToolDisplayInfo {
  const { tool_name, input } = toolCall;

  if (tool_name.startsWith("mcp:")) {
    return resolveMcpToolDisplay(toolCall);
  }

  const icon = TOOL_ICONS[tool_name] ?? Terminal;

  switch (tool_name) {
    case "file_read": {
      const filePath = typeof input.path === "string" ? input.path : "";
      const partial = "offset" in input;
      return {
        icon,
        labelKey: "tools.labels.file_read",
        descriptionKey: partial
          ? "tools.descriptions.file_read_partial"
          : "tools.descriptions.file_read",
        templateParams: { path: truncatePath(filePath) },
        outputSummaryKey: "tools.outputs.file_read",
        outputParams: resolveOutputParams(toolCall),
      };
    }

    case "file_write": {
      const filePath = typeof input.path === "string" ? input.path : "";
      return {
        icon,
        labelKey: "tools.labels.file_write",
        descriptionKey: "tools.descriptions.file_write",
        templateParams: { path: truncatePath(filePath) },
        outputSummaryKey: "tools.outputs.file_write",
        outputParams: resolveOutputParams(toolCall),
      };
    }

    case "file_edit": {
      const filePath = typeof input.path === "string" ? input.path : "";
      const replaceAll = input.replace_all === true;
      return {
        icon,
        labelKey: "tools.labels.file_edit",
        descriptionKey: replaceAll
          ? "tools.descriptions.file_edit_replace_all"
          : "tools.descriptions.file_edit",
        templateParams: { path: truncatePath(filePath) },
        outputSummaryKey: "tools.outputs.file_edit",
        outputParams: resolveOutputParams(toolCall),
      };
    }

    case "file_list": {
      const filePath = typeof input.dir === "string" ? input.dir : (typeof input.path === "string" ? input.path : "");
      const recursive = input.recursive === true;
      return {
        icon,
        labelKey: "tools.labels.file_list",
        descriptionKey: recursive
          ? "tools.descriptions.file_list_recursive"
          : "tools.descriptions.file_list",
        templateParams: { path: truncatePath(filePath) },
        outputSummaryKey: "tools.outputs.file_list",
        outputParams: resolveOutputParams(toolCall),
      };
    }

    case "file_glob": {
      const pattern =
        typeof input.pattern === "string" ? truncateString(input.pattern, 40) : "";
      const hasPath = typeof input.path === "string" && input.path.length > 0;
      const templateParams: Record<string, string> = { pattern };
      if (hasPath) {
        templateParams.path = truncatePath(input.path as string);
      }
      return {
        icon,
        labelKey: "tools.labels.file_glob",
        descriptionKey: hasPath
          ? "tools.descriptions.file_glob_in"
          : "tools.descriptions.file_glob",
        templateParams,
        outputSummaryKey: "tools.outputs.file_glob",
        outputParams: resolveOutputParams(toolCall),
      };
    }

    case "file_grep": {
      const pattern =
        typeof input.pattern === "string" ? truncateString(input.pattern, 40) : "";
      const hasGlob = typeof input.glob === "string" && input.glob.length > 0;
      const templateParams: Record<string, string> = { pattern };
      if (hasGlob) {
        templateParams.glob = input.glob as string;
      }
      return {
        icon,
        labelKey: "tools.labels.file_grep",
        descriptionKey: hasGlob
          ? "tools.descriptions.file_grep_in"
          : "tools.descriptions.file_grep",
        templateParams,
        outputSummaryKey: "tools.outputs.file_grep",
        outputParams: resolveOutputParams(toolCall),
      };
    }

    case "bash_executor": {
      const command =
        typeof input.command === "string" ? truncateString(input.command, 50) : "";
      return {
        icon,
        labelKey: "tools.labels.bash_executor",
        descriptionKey: "tools.descriptions.bash_executor",
        templateParams: { command },
        outputSummaryKey: "tools.outputs.bash_executor",
        outputParams: resolveOutputParams(toolCall),
      };
    }

    case "python_executor": {
      const code =
        typeof input.code === "string" ? truncateString(input.code, 50) : "";
      return {
        icon,
        labelKey: "tools.labels.python_executor",
        descriptionKey: "tools.descriptions.python_executor",
        templateParams: { code },
        outputSummaryKey: "tools.outputs.python_executor",
        outputParams: resolveOutputParams(toolCall),
      };
    }

    case "http_fetch": {
      const url = typeof input.url === "string" ? input.url : "";
      const method =
        typeof input.method === "string" ? input.method.toUpperCase() : "GET";
      const hostname = extractHostname(url);
      const descriptionKey =
        method === "GET"
          ? "tools.descriptions.http_fetch_get"
          : method === "POST"
            ? "tools.descriptions.http_fetch_post"
            : "tools.descriptions.http_fetch";
      return {
        icon,
        labelKey: "tools.labels.http_fetch",
        descriptionKey,
        templateParams: { hostname, method },
        outputSummaryKey: "tools.outputs.http_fetch",
        outputParams: resolveOutputParams(toolCall),
      };
    }

    case "memory_search": {
      const query =
        typeof input.query === "string" ? truncateString(input.query, 50) : "";
      const hasNamespace =
        typeof input.namespace === "string" && input.namespace.length > 0;
      const templateParams: Record<string, string> = { query };
      if (hasNamespace) {
        templateParams.namespace = input.namespace as string;
      }
      return {
        icon,
        labelKey: "tools.labels.memory_search",
        descriptionKey: hasNamespace
          ? "tools.descriptions.memory_search_ns"
          : "tools.descriptions.memory_search",
        templateParams,
        outputSummaryKey: "tools.outputs.memory_search",
        outputParams: resolveOutputParams(toolCall),
      };
    }

    case "web_search": {
      const query =
        typeof input.query === "string" ? truncateString(input.query, 50) : "";
      const requestedBackend =
        typeof input.backend === "string" && input.backend !== "auto"
          ? input.backend
          : "";
      const outputParams = resolveOutputParams(toolCall);
      // Prefer the agent-requested backend for the "searching with X for Y" label,
      // falling back to the backend the output reports having actually used.
      const backendUsed = requestedBackend || outputParams.backend || "";
      const descriptionKey = backendUsed
        ? "tools.descriptions.web_search_backend"
        : "tools.descriptions.web_search";
      const templateParams: Record<string, string> = { query };
      if (backendUsed) templateParams.backend = backendUsed;
      return {
        icon,
        labelKey: "tools.labels.web_search",
        descriptionKey,
        templateParams,
        outputSummaryKey: "tools.outputs.web_search",
        outputParams,
      };
    }

    case "web_read": {
      const url = typeof input.url === "string" ? input.url : "";
      const hostname = extractHostname(url);
      const outputParams = resolveOutputParams(toolCall);
      const outputSummaryKey =
        outputParams.truncated === "true"
          ? "tools.output.web_read_truncated"
          : "tools.outputs.web_read";
      return {
        icon,
        labelKey: "tools.labels.web_read",
        descriptionKey: "tools.descriptions.web_read",
        templateParams: { hostname },
        outputSummaryKey,
        outputParams,
      };
    }

    default: {
      return {
        icon: Terminal,
        labelKey: `tools.labels.${tool_name}`,
        descriptionKey: `tools.descriptions.${tool_name}`,
        templateParams: {},
        outputSummaryKey: null,
        outputParams: {},
      };
    }
  }
}

/**
 * Parses the JSON output of a tool call and returns a flat string map
 * of the top-level fields, suitable for i18n template interpolation.
 */
export function resolveOutputParams(toolCall: ToolCallView): Record<string, string> {
  if (!toolCall.output) return {};
  try {
    const parsed: unknown = JSON.parse(toolCall.output);
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
      return {};
    }
    const result: Record<string, string> = {};
    let firstArrayCounted = false;
    for (const [key, value] of Object.entries(parsed)) {
      if (Array.isArray(value)) {
        result[`${key}_count`] = String(value.length);
        if (!firstArrayCounted) {
          result.count = String(value.length);
          firstArrayCounted = true;
        }
      } else {
        result[key] = String(value);
      }
    }
    return result;
  } catch {
    return {};
  }
}

/**
 * Truncates a filesystem path to at most `max` characters, always preserving
 * the final filename component.
 */
export function truncatePath(path: string, max = 50): string {
  if (path.length <= max) return path;
  const sep = path.includes("/") ? "/" : "\\";
  const parts = path.split(sep);
  const filename = parts[parts.length - 1] ?? "";
  if (filename.length >= max - 3) {
    return "\u2026" + filename.slice(-(max - 3));
  }
  return "\u2026" + sep + filename;
}

/**
 * Truncates a string to at most `max` characters, appending a horizontal
 * ellipsis when truncation occurs.
 */
export function truncateString(s: string, max: number): string {
  if (s.length <= max) return s;
  return s.slice(0, max) + "\u2026";
}

/**
 * Builds a short technical summary of a tool's output for use in the builder
 * card's collapsible output toggle. Returns `null` when no meaningful summary
 * can be derived (unknown tool or missing expected output fields).
 */
export function buildOutputSummary(
  toolName: string,
  params: Record<string, string>,
): string | null {
  switch (toolName) {
    case "file_grep": {
      const matches = params.matches_count ?? params.match_count ?? params.count;
      const files = params.files_searched ?? params.file_count;
      if (matches !== undefined && files !== undefined) {
        return `${matches} matches in ${files} files`;
      }
      if (matches !== undefined) return `${matches} matches`;
      return null;
    }
    case "file_read": {
      const lines = params.total_lines;
      return lines !== undefined ? `${lines} lines` : null;
    }
    case "file_write": {
      const bytes = params.bytes_written;
      return bytes !== undefined ? `${bytes} bytes written` : null;
    }
    case "file_edit": {
      const replacements = params.replacements;
      return replacements !== undefined ? `${replacements} replacement(s)` : null;
    }
    case "file_list": {
      const count = params.entries_count ?? params.entry_count;
      return count !== undefined ? `${count} entries` : null;
    }
    case "file_glob": {
      const count = params.matches_count ?? params.match_count;
      return count !== undefined ? `${count} matches` : null;
    }
    case "bash_executor": {
      const exitCode = params.exit_code;
      return exitCode !== undefined ? `exit ${exitCode}` : null;
    }
    case "http_fetch": {
      const status = params.status_code;
      return status !== undefined ? `HTTP ${status}` : null;
    }
    case "web_search": {
      const total = params.total_results ?? params.count;
      const backend = params.backend;
      if (total !== undefined && backend) return `${total} results (${backend})`;
      if (total !== undefined) return `${total} results`;
      return null;
    }
    case "web_read": {
      const chars = params.chars_total;
      const truncated = params.truncated === "true";
      if (chars !== undefined) {
        return truncated ? `${chars} chars (truncated)` : `${chars} chars`;
      }
      return null;
    }
    case "memory_search": {
      const count = params.results_count ?? params.result_count ?? params.count;
      return count !== undefined ? `${count} results` : null;
    }
    default:
      return null;
  }
}

/**
 * Builds the `$ command` display string for a `bash_executor` input.
 * Returns `null` when the `command` field is absent or not a string.
 */
export function buildBashInputDisplay(input: Record<string, unknown>): string | null {
  const command = typeof input.command === "string" ? input.command : null;
  return command !== null ? `$ ${command}` : null;
}

/**
 * Builds the `METHOD URL` display string for an `http_fetch` input.
 * Defaults to `GET` when the `method` field is absent. Returns `null` when
 * the `url` field is absent or not a string.
 */
export function buildHttpInputDisplay(input: Record<string, unknown>): string | null {
  const url = typeof input.url === "string" ? input.url : null;
  if (url === null) return null;
  const method =
    typeof input.method === "string" ? input.method.toUpperCase() : "GET";
  return `${method} ${url}`;
}

/** Extracts the hostname from a URL string, falling back to a truncated URL on parse error. */
function extractHostname(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return truncateString(url, 30);
  }
}

/**
 * Flattens a {@link ToolCallRationale} into a display-friendly shape.
 *
 * Used by the operator-mode `ReasoningCard` tool renderer and the builder
 * rationale header. Preserves the narrator's key ordering for `inputs_recap`
 * and truncates long values so the UI stays scannable.
 */
export interface FormattedRationale {
  /** Narrative summary (<= 25 words, truncated at 160 chars). */
  summary: string;
  /** Expected outcome sentence (truncated at 160 chars). */
  expected_outcome: string;
  /** Key/value pairs with values truncated at 40 chars. */
  inputs_recap: Array<[string, string]>;
  /** Performance hint sentence if any, trimmed. */
  performance_hint: string | null;
}

export function formatRationale(
  rationale: ToolCallRationale | null | undefined,
): FormattedRationale | null {
  if (!rationale) return null;
  const recap = Array.isArray(rationale.inputs_recap)
    ? rationale.inputs_recap
        .slice(0, 4)
        .map(
          ([k, v]) => [String(k), truncateString(String(v ?? ""), 40)] as [
            string,
            string,
          ],
        )
    : [];
  return {
    summary: truncateString((rationale.summary ?? "").trim(), 160),
    expected_outcome: truncateString(
      (rationale.expected_outcome ?? "").trim(),
      160,
    ),
    inputs_recap: recap,
    performance_hint:
      typeof rationale.performance_hint === "string" &&
      rationale.performance_hint.trim().length > 0
        ? rationale.performance_hint.trim()
        : null,
  };
}

/** Resolves display info for MCP tool calls following the `mcp:server/tool` naming pattern. */
function resolveMcpToolDisplay(toolCall: ToolCallView): ToolDisplayInfo {
  const raw = toolCall.tool_name.slice("mcp:".length);
  const slashIdx = raw.indexOf("/");
  const serverName = slashIdx >= 0 ? raw.slice(0, slashIdx) : raw;
  const toolDescription = slashIdx >= 0 ? raw.slice(slashIdx + 1) : "";
  return {
    icon: Plug,
    labelKey: "tools.labels.mcp_tool",
    descriptionKey: "tools.descriptions.mcp_tool",
    templateParams: { server_name: serverName, tool_description: toolDescription },
    outputSummaryKey: null,
    outputParams: {},
  };
}
