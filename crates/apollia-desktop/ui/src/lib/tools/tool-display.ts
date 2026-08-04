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
  Mail,
  Calendar,
  HardDrive,
  Table,
  Presentation,
  ClipboardList,
  ListChecks,
  Youtube,
} from "lucide-svelte";
// NOSONAR typescript:S1874 - Svelte 4 ComponentType retained for lucide-svelte compat
import type { ComponentType } from "svelte"; // NOSONAR typescript:S1874

/**
 * Local alias for Svelte 4 `ComponentType`. Retained because lucide-svelte's
 * exported icon components are typed against this legacy alias; switching to
 * Svelte 5's `Component` would break the lucide type compatibility. All
 * `ComponentType` usages below funnel through this alias for centralized
 * suppression of the deprecated-symbol lint.
 */
// NOSONAR typescript:S1874
type SvelteComponentType = ComponentType; // NOSONAR typescript:S1874

/** Display metadata for a tool call in the chat UI. */
export interface ToolDisplayInfo {
  /** Lucide icon component for this tool. */
  icon: SvelteComponentType;
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

/** Returns the first input field whose value is a non-empty string, or "". */
function pickStringInput(
  input: Record<string, unknown>,
  ...keys: string[]
): string {
  for (const k of keys) {
    const v = input[k];
    if (typeof v === "string") return v;
  }
  return "";
}

/** Maps an HTTP method to the description key the operator card should render. */
function httpFetchDescriptionKey(method: string): string {
  if (method === "GET") return "tools.descriptions.http_fetch_get";
  if (method === "POST") return "tools.descriptions.http_fetch_post";
  return "tools.descriptions.http_fetch";
}

/**
 * Friendly brand names for the connector tool families (Google / Microsoft /
 * media). Connector tools are named `provider.action` (dot-separated), unlike
 * the underscored native tools. The prefix before the first dot picks the brand;
 * the rest is the action. Brand names are product names, kept identical across
 * locales.
 */
const CONNECTOR_LABELS: Record<string, string> = {
  gmail: "Gmail",
  gcal: "Google Calendar",
  gdrive: "Google Drive",
  gdocs: "Google Docs",
  gsheets: "Google Sheets",
  gslides: "Google Slides",
  gforms: "Google Forms",
  gtasks: "Google Tasks",
  youtube: "YouTube",
  outlook: "Outlook",
  outlook_cal: "Outlook Calendar",
  onedrive: "OneDrive",
};

/** Lucide icon per connector family, falling back to the generic plug. */
const CONNECTOR_ICONS: Record<string, SvelteComponentType> = {
  gmail: Mail,
  gcal: Calendar,
  gdrive: HardDrive,
  gdocs: FileText,
  gsheets: Table,
  gslides: Presentation,
  gforms: ClipboardList,
  gtasks: ListChecks,
  youtube: Youtube,
  outlook: Mail,
  outlook_cal: Calendar,
  onedrive: HardDrive,
};

/** Splits a `provider.action` connector name into its brand and humanized action. */
function splitConnector(toolName: string): { provider: string; action: string } {
  const dot = toolName.indexOf(".");
  const prefix = dot >= 0 ? toolName.slice(0, dot) : toolName;
  const rest = dot >= 0 ? toolName.slice(dot + 1) : "";
  const provider = CONNECTOR_LABELS[prefix] ?? titleCaseWords(prefix);
  const action = rest ? rest.replace(/[._]/g, " ") : "";
  return { provider, action };
}

/** Title-cases an underscore/dot separated identifier for a readable fallback. */
function titleCaseWords(raw: string): string {
  return raw
    .split(/[._]/)
    .filter(Boolean)
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

/**
 * Human-readable fallback name for any tool that lacks a dedicated i18n label,
 * so a missing translation key never surfaces as a raw `tools.descriptions.*`
 * string. Connector tools resolve to their brand plus action; every other tool
 * is title-cased from its identifier.
 */
export function humanizeToolName(toolName: string): string {
  if (toolName.startsWith("mcp:")) {
    const raw = toolName.slice("mcp:".length);
    return titleCaseWords(raw.replace("/", " "));
  }
  if (toolName.includes(".")) {
    const { provider, action } = splitConnector(toolName);
    return action ? `${provider}: ${action}` : provider;
  }
  return titleCaseWords(toolName);
}

/** Resolves display info for a `provider.action` connector tool. */
function resolveConnectorDisplay(toolCall: ToolCallView): ToolDisplayInfo {
  const dot = toolCall.tool_name.indexOf(".");
  const prefix = dot >= 0 ? toolCall.tool_name.slice(0, dot) : toolCall.tool_name;
  const { provider, action } = splitConnector(toolCall.tool_name);
  return {
    icon: CONNECTOR_ICONS[prefix] ?? Plug,
    labelKey: "tools.labels.connector",
    descriptionKey: "tools.descriptions.connector",
    templateParams: { provider, action },
    outputSummaryKey: null,
    outputParams: resolveOutputParams(toolCall),
  };
}

export const TOOL_ICONS: Record<string, SvelteComponentType> = {
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
type ToolResolver = (
  toolCall: ToolCallView,
  icon: SvelteComponentType,
) => ToolDisplayInfo;

function resolveFileRead(toolCall: ToolCallView, icon: SvelteComponentType): ToolDisplayInfo {
  const input = toolCall.input;
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

function resolveFileWrite(toolCall: ToolCallView, icon: SvelteComponentType): ToolDisplayInfo {
  const filePath = typeof toolCall.input.path === "string" ? toolCall.input.path : "";
  return {
    icon,
    labelKey: "tools.labels.file_write",
    descriptionKey: "tools.descriptions.file_write",
    templateParams: { path: truncatePath(filePath) },
    outputSummaryKey: "tools.outputs.file_write",
    outputParams: resolveOutputParams(toolCall),
  };
}

function resolveFileEdit(toolCall: ToolCallView, icon: SvelteComponentType): ToolDisplayInfo {
  const input = toolCall.input;
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

function resolveFileList(toolCall: ToolCallView, icon: SvelteComponentType): ToolDisplayInfo {
  const input = toolCall.input;
  const filePath = pickStringInput(input, "dir", "path");
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

function resolveFileGlob(toolCall: ToolCallView, icon: SvelteComponentType): ToolDisplayInfo {
  const input = toolCall.input;
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

function resolveFileGrep(toolCall: ToolCallView, icon: SvelteComponentType): ToolDisplayInfo {
  const input = toolCall.input;
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

function resolveBashExecutor(toolCall: ToolCallView, icon: SvelteComponentType): ToolDisplayInfo {
  const input = toolCall.input;
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

function resolvePythonExecutor(toolCall: ToolCallView, icon: SvelteComponentType): ToolDisplayInfo {
  const input = toolCall.input;
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

function resolveHttpFetch(toolCall: ToolCallView, icon: SvelteComponentType): ToolDisplayInfo {
  const input = toolCall.input;
  const url = typeof input.url === "string" ? input.url : "";
  const method =
    typeof input.method === "string" ? input.method.toUpperCase() : "GET";
  const hostname = extractHostname(url);
  return {
    icon,
    labelKey: "tools.labels.http_fetch",
    descriptionKey: httpFetchDescriptionKey(method),
    templateParams: { hostname, method },
    outputSummaryKey: "tools.outputs.http_fetch",
    outputParams: resolveOutputParams(toolCall),
  };
}

function resolveMemorySearch(toolCall: ToolCallView, icon: SvelteComponentType): ToolDisplayInfo {
  const input = toolCall.input;
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

function resolveWebSearch(toolCall: ToolCallView, icon: SvelteComponentType): ToolDisplayInfo {
  const input = toolCall.input;
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

function resolveWebRead(toolCall: ToolCallView, icon: SvelteComponentType): ToolDisplayInfo {
  const input = toolCall.input;
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

const TOOL_RESOLVERS: Record<string, ToolResolver> = {
  file_read: resolveFileRead,
  file_write: resolveFileWrite,
  file_edit: resolveFileEdit,
  file_list: resolveFileList,
  file_glob: resolveFileGlob,
  file_grep: resolveFileGrep,
  bash_executor: resolveBashExecutor,
  python_executor: resolvePythonExecutor,
  http_fetch: resolveHttpFetch,
  memory_search: resolveMemorySearch,
  web_search: resolveWebSearch,
  web_read: resolveWebRead,
};

export function resolveToolDisplay(toolCall: ToolCallView): ToolDisplayInfo {
  const { tool_name } = toolCall;

  if (tool_name.startsWith("mcp:")) {
    return resolveMcpToolDisplay(toolCall);
  }

  const icon = TOOL_ICONS[tool_name] ?? Terminal;
  const resolver = TOOL_RESOLVERS[tool_name];
  if (resolver) return resolver(toolCall, icon);

  // Connector tools (`provider.action`, dot-separated) get a friendly brand +
  // action rendering instead of a raw i18n key.
  if (tool_name.includes(".")) {
    return resolveConnectorDisplay(toolCall);
  }

  return {
    icon: TOOL_ICONS[tool_name] ?? Terminal,
    labelKey: `tools.labels.${tool_name}`,
    descriptionKey: `tools.descriptions.${tool_name}`,
    templateParams: {},
    outputSummaryKey: null,
    outputParams: {},
  };
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
  const filename = parts.at(-1) ?? "";
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
type OutputSummariser = (params: Record<string, string>) => string | null;

const OUTPUT_SUMMARISERS: Record<string, OutputSummariser> = {
  file_grep: (params) => {
    const matches = params.matches_count ?? params.match_count ?? params.count;
    const files = params.files_searched ?? params.file_count;
    if (matches !== undefined && files !== undefined) {
      return `${matches} matches in ${files} files`;
    }
    return matches === undefined ? null : `${matches} matches`;
  },
  file_read: (params) => {
    const lines = params.total_lines;
    return lines === undefined ? null : `${lines} lines`;
  },
  file_write: (params) => {
    const bytes = params.bytes_written;
    return bytes === undefined ? null : `${bytes} bytes written`;
  },
  file_edit: (params) => {
    const replacements = params.replacements;
    return replacements === undefined ? null : `${replacements} replacement(s)`;
  },
  file_list: (params) => {
    const count = params.entries_count ?? params.entry_count;
    return count === undefined ? null : `${count} entries`;
  },
  file_glob: (params) => {
    const count = params.matches_count ?? params.match_count;
    return count === undefined ? null : `${count} matches`;
  },
  bash_executor: (params) => {
    const exitCode = params.exit_code;
    return exitCode === undefined ? null : `exit ${exitCode}`;
  },
  http_fetch: (params) => {
    const status = params.status_code;
    return status === undefined ? null : `HTTP ${status}`;
  },
  web_search: (params) => {
    const total = params.total_results ?? params.count;
    if (total === undefined) return null;
    const backend = params.backend;
    return backend ? `${total} results (${backend})` : `${total} results`;
  },
  web_read: (params) => {
    const chars = params.chars_total;
    if (chars === undefined) return null;
    const truncated = params.truncated === "true";
    return truncated ? `${chars} chars (truncated)` : `${chars} chars`;
  },
  memory_search: (params) => {
    const count = params.results_count ?? params.result_count ?? params.count;
    return count === undefined ? null : `${count} results`;
  },
};

export function buildOutputSummary(
  toolName: string,
  params: Record<string, string>,
): string | null {
  const summariser = OUTPUT_SUMMARISERS[toolName];
  return summariser ? summariser(params) : null;
}

/**
 * The readable part of a failed tool's output.
 *
 * The runtime prefixes a failure with `tool error: ` and the dispatcher adds
 * its error code before the message, so an operator was shown
 * `tool error: google: no Google account connected` where the sentence they
 * need starts at the third word. Both prefixes are machine framing; the
 * builder layer still shows the raw output verbatim.
 */
export function humanizeToolError(output: string): string {
  let text = output.trim();
  const prefix = "tool error:";
  if (text.toLowerCase().startsWith(prefix)) {
    text = text.slice(prefix.length).trim();
  }
  // `<code>: <message>`, where the code is a machine token (`no_account`,
  // `spawn_failed`, `google`). A colon inside a sentence is left alone.
  const colon = text.indexOf(":");
  if (colon > 0 && /^[a-z][a-z0-9_.]*$/.test(text.slice(0, colon))) {
    text = text.slice(colon + 1).trim();
  }
  return text;
}

/**
 * A one-line reading of an arbitrary tool output, for tools with no bespoke
 * body: the connectors, the MCP servers, the governance tools.
 *
 * Returns `null` when the output carries no readable shape, so the caller can
 * decide what to show rather than being handed a JSON dump. An operator faced
 * with `{"resources":[]}` learns nothing; "no result" is the same fact, said.
 */
export function summariseJsonOutput(output: string): string | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(output);
  } catch {
    return null;
  }
  if (Array.isArray(parsed)) {
    return parsed.length === 0 ? "empty" : `${parsed.length}`;
  }
  if (parsed === null || typeof parsed !== "object") return null;
  const record = parsed as Record<string, unknown>;
  // The first array-valued field is the payload in every connector and MCP
  // response shape we ship: `results`, `resources`, `rules`, `files`, `items`.
  for (const value of Object.values(record)) {
    if (Array.isArray(value)) {
      return value.length === 0 ? "empty" : `${value.length}`;
    }
  }
  return Object.keys(record).length === 0 ? "empty" : null;
}

/**
 * Builds the `$ command` display string for a `bash_executor` input.
 * Returns `null` when the `command` field is absent or not a string.
 */
export function buildBashInputDisplay(input: Record<string, unknown>): string | null {
  const command = typeof input.command === "string" ? input.command : null;
  return command === null ? null : `$ ${command}`;
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
