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

/** Basename of a filesystem path, tolerant of both separators. */
export function basename(path: string): string {
  const sep = path.includes("/") ? "/" : "\\";
  const parts = path.split(sep).filter((p) => p.length > 0);
  return parts.at(-1) ?? path;
}
