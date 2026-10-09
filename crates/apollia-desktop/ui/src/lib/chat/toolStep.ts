/**
 * What one tool step of an assistant turn says on its folded line: the action
 * in plain words, a short target, a state and a duration.
 *
 * The row is the same for every tool. Only the action name, the target and the
 * icon change, and they are decided here, away from the markup, so the rules
 * are testable without a rendered tree.
 */

import type { ToolCallView } from "$lib/types";
import {
  ArrowLeftRight,
  BookOpen,
  Code,
  Database,
  UserPen,
  FilePen,
  FileSearch,
  FileText,
  Globe,
  List,
  ListChecks,
  SquareTerminal,
  TextSearch,
} from "lucide-svelte";
import { resolveToolDisplay, type ToolDisplayInfo } from "$lib/tools/tool-display";
import type { ReasoningItem, ReasoningStatus } from "./reasoning";
import { basename, parseTodos } from "./toolBodies";

type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

/** The four states a step shows. */
export type StepState = "running" | "success" | "error" | "refused";

/** Map a reasoning status onto the state the step row shows. */
export function stepState(status: ReasoningStatus): StepState {
  switch (status) {
    case "success":
    case "approved":
      return "success";
    case "error":
      return "error";
    case "rejected":
      return "refused";
    default:
      return "running";
  }
}

/**
 * The plain-language action of each tool that owns a refreshed body. A tool
 * missing here takes its existing label (connectors, external tools).
 */
const ACTION_KEYS: Record<string, string> = {
  web_search: "tools.actions.web_search",
  web_read: "tools.actions.web_read",
  file_list: "tools.actions.file_list",
  file_read: "tools.actions.file_read",
  file_write: "tools.actions.file_write",
  file_edit: "tools.actions.file_edit",
  file_glob: "tools.actions.file_glob",
  file_grep: "tools.actions.file_grep",
  bash_executor: "tools.actions.bash_executor",
  python_executor: "tools.actions.python_executor",
  http_fetch: "tools.actions.http_fetch",
  memory_search: "tools.actions.memory_search",
  memory_write: "tools.actions.memory_write",
  profile_propose: "tools.actions.profile_propose",
  todo_write: "tools.actions.todo_write",
};

/** The catalogue key of a tool's action name, or `null` when it has none. */
export function stepActionKey(tool: string): string | null {
  return ACTION_KEYS[tool] ?? null;
}

/** The longest target the folded line shows before it is shortened. */
export const TARGET_MAX = 60;

/** A short target: plain text, or a catalogue key with its values. */
export type StepTarget =
  | { text: string }
  | { key: string; values: Record<string, number> }
  | null;

function shorten(text: string, max = TARGET_MAX): string {
  const one = text.replace(/\s+/g, " ").trim();
  return one.length > max ? `${one.slice(0, max - 1)}…` : one;
}

function str(args: Record<string, unknown>, ...keys: string[]): string {
  for (const k of keys) {
    const v = args[k];
    if (typeof v === "string" && v.trim()) return v;
  }
  return "";
}

/** The host of a URL without `www.`; the input itself when it does not parse. */
export function hostOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return url;
  }
}

/** The last two segments of a path, the part a reader recognises. */
export function shortPath(path: string): string {
  const parts = path.split(/[\\/]/).filter((p) => p.length > 0);
  return parts.slice(-2).join("/") || path;
}

function textTarget(text: string): StepTarget {
  return text ? { text: shorten(text) } : null;
}

/** The short target a step names after its action, from the call's arguments. */
export function stepTarget(item: ToolCallItem): StepTarget {
  const args = item.args ?? {};
  switch (item.tool) {
    case "web_search":
    case "memory_search":
      return textTarget(str(args, "query"));
    case "memory_write":
    case "profile_propose":
      return textTarget(str(args, "key"));
    case "web_read":
    case "http_fetch": {
      const url = str(args, "url");
      return textTarget(url ? hostOf(url) : "");
    }
    case "file_list":
      return textTarget(shortPath(str(args, "dir", "path")));
    case "file_read":
    case "file_write":
    case "file_edit": {
      const path = str(args, "path");
      return textTarget(path ? basename(path) : "");
    }
    case "file_glob":
    case "file_grep":
      return textTarget(str(args, "pattern"));
    case "bash_executor":
      return textTarget(str(args, "command").split("\n")[0] ?? "");
    case "python_executor": {
      const code = str(args, "code");
      if (!code) return null;
      const n = code.replace(/\n+$/, "").split("\n").length;
      return { key: "tools.body.code_lines", values: { n } };
    }
    case "todo_write": {
      const todos = parseTodos(args);
      if (!todos) return null;
      const done = todos.filter((t) => t.status === "completed").length;
      return { key: "tools.body.done_of_total", values: { done, total: todos.length } };
    }
    default:
      return textTarget(firstStringArgument(args));
  }
}

/** The first short string argument of a call, for a tool with no rule above. */
function firstStringArgument(args: Record<string, unknown>): string {
  for (const v of Object.values(args)) {
    if (typeof v === "string" && v.trim() && v.length <= 200) return v;
  }
  return "";
}

/**
 * The duration a reasoning block is titled with: whole seconds, at least one,
 * then minutes and seconds past a minute. The units read the same in both
 * catalogues.
 */
export function formatThoughtDuration(ms: number): string {
  const total = Math.max(1, Math.round(ms / 1000));
  if (total < 60) return `${total} s`;
  const min = Math.floor(total / 60);
  const sec = total % 60;
  return sec === 0 ? `${min} min` : `${min} min ${sec} s`;
}

/** The runtime view of a step, as the shared tool display helpers read it. */
export function toolCallView(item: ToolCallItem): ToolCallView {
  return {
    tool_name: item.tool,
    input: item.args,
    output: item.output,
    status:
      item.status === "success"
        ? "executed"
        : item.status === "error"
          ? "failed"
          : item.status === "rejected"
            ? "refused"
            : item.status === "running" || item.status === "approved"
              ? "authorized"
              : "pending",
    duration_ms: item.duration_ms,
    exit_code: item.exit_code,
  };
}

/** Arguments a generic body lists at most. */
export const PARAMS_MAX = 8;

/** One argument of a call, its value made short and readable. */
export function stepParameters(args: Record<string, unknown>): [string, string][] {
  const out: [string, string][] = [];
  for (const [key, value] of Object.entries(args ?? {})) {
    if (value === null || value === undefined || value === "") continue;
    let text: string;
    if (typeof value === "string") text = value;
    else if (typeof value === "number" || typeof value === "boolean") text = String(value);
    else {
      try {
        text = JSON.stringify(value);
      } catch {
        text = String(value);
      }
    }
    out.push([key, shorten(text, 120)]);
    if (out.length >= PARAMS_MAX) break;
  }
  return out;
}

const STEP_ICONS: Record<string, ToolDisplayInfo["icon"]> = {
  web_search: Globe,
  web_read: BookOpen,
  file_list: List,
  file_read: FileText,
  file_write: FilePen,
  file_edit: FilePen,
  file_glob: FileSearch,
  file_grep: TextSearch,
  bash_executor: SquareTerminal,
  python_executor: Code,
  http_fetch: ArrowLeftRight,
  memory_search: Database,
  memory_write: Database,
  profile_propose: UserPen,
  todo_write: ListChecks,
};

/** The icon of a step: the tool's own, else the one its display resolves. */
export function stepIcon(item: ToolCallItem): ToolDisplayInfo["icon"] {
  return STEP_ICONS[item.tool] ?? resolveToolDisplay(toolCallView(item)).icon;
}

/** The icon of a tool known by name only, as a live step is. */
export function stepIconByName(tool: string): ToolDisplayInfo["icon"] {
  return (
    STEP_ICONS[tool] ??
    resolveToolDisplay({ tool_name: tool, input: {}, output: null, status: "executed", duration_ms: null }).icon
  );
}
