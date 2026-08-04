/**
 * Parsing and formatting helpers for the per-tool expanded bodies rendered by
 * `ReasoningCard` (see `components/chat/tool-bodies/`).
 *
 * Each helper is pure and defensive: raw tool output is untrusted and often
 * malformed, so parsers return `null` (or a safe fallback) instead of throwing,
 * letting the body component degrade gracefully to the raw string.
 */

/** One entry of a `file_list` result, normalized for the operator listing. */
export interface FileListEntry {
  name: string;
  /** `"dir"` marks a folder; anything else renders with the file icon. */
  type: string;
  /** Byte size for files, when the runtime reported one. */
  size?: number | null;
  /** Child-entry count for folders, when the runtime reported one. */
  entries?: number | null;
}

/** A size expressed as a magnitude plus a unit bucket, for localized display. */
export interface HumanSize {
  /** Magnitude in the chosen unit, rounded to one decimal for K and above. */
  value: number;
  unit: "bytes" | "kb" | "mb" | "gb";
}

const RAW_ARRAY_KEYS = ["entries", "files", "items", "results"] as const;

function coerceEntry(raw: unknown): FileListEntry | null {
  if (typeof raw !== "object" || raw === null) return null;
  const obj = raw as Record<string, unknown>;
  const rawName =
    typeof obj.name === "string"
      ? obj.name
      : typeof obj.path === "string"
        ? obj.path
        : null;
  if (rawName === null) return null;
  const sep = rawName.includes("/") ? "/" : "\\";
  const parts = rawName.split(sep).filter((p) => p.length > 0);
  const name = parts.at(-1) ?? rawName;
  const type =
    typeof obj.type === "string"
      ? obj.type
      : obj.is_dir === true || obj.directory === true
        ? "dir"
        : "file";
  const size =
    typeof obj.size === "number"
      ? obj.size
      : typeof obj.bytes === "number"
        ? obj.bytes
        : null;
  const entries =
    typeof obj.entries === "number"
      ? obj.entries
      : typeof obj.count === "number"
        ? obj.count
        : null;
  return { name, type, size, entries };
}

/**
 * Recover a `file_list` entry array from a tool call's raw output.
 *
 * Accepts either a bare JSON array or an object wrapping the array under a
 * common key (`entries`, `files`, `items`, `results`). Returns `null` when the
 * output is absent, unparseable, or carries no usable entries, so the body can
 * fall back to the raw string.
 */
export function parseFileList(output: string | null): FileListEntry[] | null {
  if (!output) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(output);
  } catch {
    return null;
  }
  let rawArray: unknown[] | null = null;
  if (Array.isArray(parsed)) {
    rawArray = parsed;
  } else if (typeof parsed === "object" && parsed !== null) {
    const obj = parsed as Record<string, unknown>;
    for (const key of RAW_ARRAY_KEYS) {
      if (Array.isArray(obj[key])) {
        rawArray = obj[key] as unknown[];
        break;
      }
    }
  }
  if (!rawArray) return null;
  const entries = rawArray
    .map(coerceEntry)
    .filter((e): e is FileListEntry => e !== null);
  return entries.length > 0 ? entries : null;
}

/** True when an entry should render with the folder icon and affordances. */
export function isFolder(entry: FileListEntry): boolean {
  return entry.type === "dir" || entry.type === "directory";
}

/**
 * Convert a byte count into a magnitude + unit bucket for localized rendering.
 *
 * Bytes stay whole; kibibyte and above are rounded to one decimal. The caller
 * formats `value` through `Intl.NumberFormat` and maps `unit` to a localized
 * label, so this stays locale-agnostic.
 */
export function humanSize(bytes: number): HumanSize {
  if (!Number.isFinite(bytes) || bytes < 1024) {
    return { value: Math.max(0, Math.round(bytes)), unit: "bytes" };
  }
  const kb = bytes / 1024;
  if (kb < 1024) return { value: Math.round(kb * 10) / 10, unit: "kb" };
  const mb = kb / 1024;
  if (mb < 1024) return { value: Math.round(mb * 10) / 10, unit: "mb" };
  return { value: Math.round((mb / 1024) * 10) / 10, unit: "gb" };
}

/**
 * Pretty-print a raw output string when it is JSON, so builder-mode bodies are
 * indented and readable. Non-JSON output is returned untouched.
 */
export function prettyJson(raw: string | null): string {
  if (!raw) return "";
  const trimmed = raw.trim();
  if (!(trimmed.startsWith("{") || trimmed.startsWith("["))) return raw;
  try {
    return JSON.stringify(JSON.parse(trimmed), null, 2);
  } catch {
    return raw;
  }
}

/** Serialize a value to pretty JSON, falling back to `String()` on cycles. */
export function stringifyValue(value: unknown): string {
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

/**
 * Split a bash tool output into stdout/stderr fragments and an exit code.
 *
 * The runtime serializes `bash_executor` output as JSON
 * (`{stdout, stderr, exit_code}`) but older/plain outputs are bare text. This
 * always yields a printable body plus, when available, a structured exit code.
 */
export interface BashOutput {
  body: string;
  stderr: string;
  exitCode: number | null;
}

export function parseBashOutput(
  output: string | null,
  fallbackExit: number | null | undefined,
): BashOutput {
  if (!output) {
    return { body: "", stderr: "", exitCode: fallbackExit ?? null };
  }
  const trimmed = output.trim();
  if (trimmed.startsWith("{")) {
    try {
      const parsed = JSON.parse(trimmed) as {
        stdout?: unknown;
        stderr?: unknown;
        output?: unknown;
        exit_code?: unknown;
      };
      const stdout =
        typeof parsed.stdout === "string"
          ? parsed.stdout
          : typeof parsed.output === "string"
            ? parsed.output
            : "";
      const stderr = typeof parsed.stderr === "string" ? parsed.stderr : "";
      const exitCode =
        typeof parsed.exit_code === "number"
          ? parsed.exit_code
          : (fallbackExit ?? null);
      return { body: stdout, stderr, exitCode };
    } catch {
      // fall through to the plain-text branch
    }
  }
  return { body: output, stderr: "", exitCode: fallbackExit ?? null };
}

/** Count the non-empty lines of a tool output, for the operator summary. */
export function countOutputLines(output: string | null): number {
  if (!output) return 0;
  return output.split("\n").filter((l) => l.trim().length > 0).length;
}

/**
 * Maps a shell command to the i18n key of a plain-language description of what
 * it does, so operators (who do not read shell) understand a `bash_executor`
 * call without seeing the raw command. Recognizes the leading program name,
 * skipping common prefixes (`sudo`, env assignments). Returns
 * `tools.body.bash_describe.generic` when the program is not recognized.
 */
const BASH_VERB_KEYS: Record<string, string> = {
  ls: "list_files",
  dir: "list_files",
  tree: "list_files",
  find: "find_files",
  fd: "find_files",
  cat: "read_file",
  bat: "read_file",
  less: "read_file",
  more: "read_file",
  head: "read_file",
  tail: "read_file",
  grep: "search_text",
  rg: "search_text",
  ag: "search_text",
  mkdir: "create_folder",
  touch: "create_file",
  rm: "delete",
  rmdir: "delete",
  unlink: "delete",
  mv: "move",
  cp: "copy",
  echo: "print_text",
  printf: "print_text",
  cd: "navigate",
  pwd: "navigate",
  curl: "download",
  wget: "download",
  git: "version_control",
  python: "run_program",
  python3: "run_program",
  node: "run_program",
  npm: "run_program",
  pnpm: "run_program",
  yarn: "run_program",
  pip: "run_program",
  pip3: "run_program",
  cargo: "run_program",
  make: "run_program",
  chmod: "change_permissions",
  chown: "change_permissions",
  tar: "archive",
  zip: "archive",
  unzip: "archive",
  gzip: "archive",
  ps: "process_management",
  top: "process_management",
  kill: "process_management",
  df: "disk_usage",
  du: "disk_usage",
  wc: "count_lines",
  sort: "transform_text",
  uniq: "transform_text",
  sed: "transform_text",
  awk: "transform_text",
  cut: "transform_text",
};

export function describeBashCommand(command: string): string {
  const base = "tools.body.bash_describe";
  const trimmed = command.trim();
  if (!trimmed) return `${base}.generic`;
  const tokens = trimmed.split(/\s+/);
  let i = 0;
  // Skip a leading `sudo` and `KEY=value` environment assignments.
  while (i < tokens.length && (tokens[i] === "sudo" || /^[A-Za-z_][A-Za-z0-9_]*=/.test(tokens[i]!))) {
    i += 1;
  }
  const program = (tokens[i] ?? "").split("/").at(-1) ?? "";
  const verb = BASH_VERB_KEYS[program];
  return verb ? `${base}.${verb}` : `${base}.generic`;
}

/** Basename of a filesystem path, tolerant of both separators. */
export function basename(path: string): string {
  const sep = path.includes("/") ? "/" : "\\";
  const parts = path.split(sep).filter((p) => p.length > 0);
  return parts.at(-1) ?? path;
}

/** Directory portion of a path (everything before the basename), or "". */
export function dirname(path: string): string {
  const sep = path.includes("/") ? "/" : "\\";
  const parts = path.split(sep).filter((p) => p.length > 0);
  if (parts.length <= 1) return "";
  return parts.slice(0, -1).join(sep);
}

/** Parse an unknown string as a JSON object, returning `null` on any failure. */
function parseJsonObject(output: string | null): Record<string, unknown> | null {
  if (!output) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(output);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    return null;
  }
  return parsed as Record<string, unknown>;
}

/**
 * Recover a `file_glob` match list from a tool call's raw output.
 *
 * The runtime serializes `{ matches: string[] }`, but a bare array is also
 * accepted. Returns `null` when the output is absent, unparseable, or empty so
 * the body degrades to the raw string.
 */
export function parseFileGlob(output: string | null): string[] | null {
  if (!output) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(output);
  } catch {
    return null;
  }
  let raw: unknown[] | null = null;
  if (Array.isArray(parsed)) {
    raw = parsed;
  } else if (typeof parsed === "object" && parsed !== null) {
    const m = (parsed as Record<string, unknown>).matches;
    if (Array.isArray(m)) raw = m;
  }
  if (!raw) return null;
  // An empty match list is a result, not a parse failure. Returning null here
  // sent the body to its raw-output fallback, so a search that found nothing
  // was shown to an operator as `{"matches":[]}`.
  return raw.filter((p): p is string => typeof p === "string" && p.length > 0);
}

/** One matched line inside a `file_grep` file group. */
export interface GrepRow {
  line: number;
  text: string;
}

/** All `file_grep` matches that fall inside a single file. */
export interface GrepFileGroup {
  file: string;
  rows: GrepRow[];
}

/** Normalized `file_grep` result: matches grouped by file plus the counters. */
export interface GrepParsed {
  groups: GrepFileGroup[];
  totalMatches: number;
  filesSearched: number;
  truncated: boolean;
}

/**
 * Recover a `file_grep` result from a tool call's raw output.
 *
 * The runtime serializes
 * `{ matches: [{ file, line_number, content, ... }], truncated, files_searched }`.
 * Matches are grouped by file, preserving first-seen order. Returns `null` when
 * the shape does not match so the body falls back to the raw output.
 */
export function parseGrep(output: string | null): GrepParsed | null {
  const obj = parseJsonObject(output);
  if (!obj || !Array.isArray(obj.matches)) return null;
  const order: string[] = [];
  const byFile = new Map<string, GrepRow[]>();
  for (const raw of obj.matches) {
    if (typeof raw !== "object" || raw === null) continue;
    const m = raw as Record<string, unknown>;
    const file = typeof m.file === "string" ? m.file : "";
    const line =
      typeof m.line_number === "number"
        ? m.line_number
        : typeof m.line === "number"
          ? m.line
          : 0;
    const text =
      typeof m.content === "string"
        ? m.content
        : typeof m.text === "string"
          ? m.text
          : "";
    if (!file) continue;
    if (!byFile.has(file)) {
      byFile.set(file, []);
      order.push(file);
    }
    byFile.get(file)?.push({ line, text });
  }
  // No match is a result an operator can read ("0 matches in 12 files"). Only
  // an output that is not a grep result at all returns null, above.
  const groups = order.map((file) => ({ file, rows: byFile.get(file) ?? [] }));
  const totalMatches = groups.reduce((n, g) => n + g.rows.length, 0);
  const filesSearched =
    typeof obj.files_searched === "number" ? obj.files_searched : order.length;
  return {
    groups,
    totalMatches,
    filesSearched,
    truncated: obj.truncated === true,
  };
}

/** Normalized `http_fetch` response, with content-type and size teased out. */
export interface HttpParsed {
  status: number;
  headers: Record<string, string>;
  body: string;
  durationMs: number | null;
  contentType: string | null;
  byteSize: number | null;
}

/** Coarse status class for the operator badge colour and label. */
export type HttpStatusClass =
  | "info"
  | "success"
  | "redirect"
  | "client"
  | "server"
  | "unknown";

/** Map an HTTP status code to its coarse class bucket. */
export function httpStatusClass(status: number): HttpStatusClass {
  if (status >= 100 && status < 200) return "info";
  if (status >= 200 && status < 300) return "success";
  if (status >= 300 && status < 400) return "redirect";
  if (status >= 400 && status < 500) return "client";
  if (status >= 500 && status < 600) return "server";
  return "unknown";
}

/**
 * Recover an `http_fetch` response from a tool call's raw output.
 *
 * The runtime serializes `{ status, headers, body, duration_ms }`. Header
 * lookups are case-insensitive so `content-type` / `Content-Length` are found
 * regardless of casing. Returns `null` when no numeric `status` is present.
 */
export function parseHttp(output: string | null): HttpParsed | null {
  const obj = parseJsonObject(output);
  if (!obj || typeof obj.status !== "number") return null;
  const headers: Record<string, string> = {};
  if (typeof obj.headers === "object" && obj.headers !== null) {
    for (const [k, v] of Object.entries(obj.headers)) {
      if (typeof v === "string") headers[k] = v;
    }
  }
  const lower = new Map(
    Object.entries(headers).map(([k, v]) => [k.toLowerCase(), v]),
  );
  const contentType = lower.get("content-type") ?? null;
  const body = typeof obj.body === "string" ? obj.body : "";
  const lenHeader = lower.get("content-length");
  const byteSize =
    lenHeader !== undefined && /^\d+$/.test(lenHeader)
      ? Number(lenHeader)
      : body
        ? new TextEncoder().encode(body).length
        : null;
  return {
    status: obj.status,
    headers,
    body,
    durationMs: typeof obj.duration_ms === "number" ? obj.duration_ms : null,
    contentType: contentType ? contentType.split(";")[0].trim() : null,
    byteSize,
  };
}

/** One recalled entry of a `memory_search` result. */
export interface MemoryEntry {
  content: string;
  source: string;
  score: number | null;
  relevance: number | null;
  createdAt: string | null;
}

/** Normalized `memory_search` result. */
export interface MemoryParsed {
  entries: MemoryEntry[];
  totalFound: number;
}

/**
 * Recover a `memory_search` result from a tool call's raw output.
 *
 * The runtime serializes
 * `{ results: [{ score, source, content, relevance?, created_at }], total_found }`.
 * Returns `null` when no usable entry is present.
 */
export function parseMemory(output: string | null): MemoryParsed | null {
  const obj = parseJsonObject(output);
  if (!obj || !Array.isArray(obj.results)) return null;
  const entries: MemoryEntry[] = [];
  for (const raw of obj.results) {
    if (typeof raw !== "object" || raw === null) continue;
    const r = raw as Record<string, unknown>;
    if (typeof r.content !== "string" || r.content.length === 0) continue;
    entries.push({
      content: r.content,
      source: typeof r.source === "string" ? r.source : "",
      score: typeof r.score === "number" ? r.score : null,
      relevance: typeof r.relevance === "number" ? r.relevance : null,
      createdAt: typeof r.created_at === "string" ? r.created_at : null,
    });
  }
  // A search that recalled nothing is a readable outcome; the raw
  // `{"results":[],"total_found":0}` an operator used to be shown is not.
  const totalFound =
    typeof obj.total_found === "number" ? obj.total_found : entries.length;
  return { entries, totalFound };
}

/** One item of a `todo_write` list, normalized for the operator rendering. */
export interface TodoItemLite {
  id: string;
  content: string;
  status: string;
}

/**
 * Recover the `todo_write` list from a tool call's arguments.
 *
 * The full list travels in `args.items` (`{ id, content, status }`); the output
 * only carries a count. Returns `null` when no usable item is present.
 */
export function parseTodos(
  args: Record<string, unknown>,
): TodoItemLite[] | null {
  const raw = args.items;
  if (!Array.isArray(raw)) return null;
  const items: TodoItemLite[] = [];
  for (const entry of raw) {
    if (typeof entry !== "object" || entry === null) continue;
    const o = entry as Record<string, unknown>;
    if (typeof o.content !== "string" || o.content.length === 0) continue;
    items.push({
      id: typeof o.id === "string" ? o.id : "",
      content: o.content,
      status: typeof o.status === "string" ? o.status : "pending",
    });
  }
  return items.length > 0 ? items : null;
}

/** Total line count of a text blob (unlike `countOutputLines`, counts blanks). */
export function totalLines(text: string | null): number {
  if (!text) return 0;
  return text.replace(/\n$/, "").split("\n").length;
}
