/**
 * Artifact detection heuristics (US-SP42-031).
 *
 * v1: runs over the message list of a session. Any tool-role message whose
 * content is long enough or originates from an artifact-shaped tool becomes
 * an artifact. Heuristic runs client-side to avoid reshaping the runtime
 * event bus for the v1 — the backend side (full content in ChatToolCall
 * events) is deferred to v2.
 */
import type { ChatMessageView } from "$lib/types";
import { saveArtifact, type Artifact } from "$lib/stores/artifacts";

/** Tools whose output is always treated as an artifact, regardless of length. */
const ARTIFACT_TOOLS = new Set<string>([
  "file_write",
  "write_file",
  "save_file",
  "edit_file",
  "notebook",
  "notebook_edit",
]);

/** Minimum number of lines for length-based detection. */
const MIN_LINES = 20;

function guessLanguage(
  tool: string | null | undefined,
  title: string,
): string | null {
  const t = (title || "").toLowerCase();
  if (t.endsWith(".rs")) return "rust";
  if (t.endsWith(".ts") || t.endsWith(".tsx")) return "typescript";
  if (t.endsWith(".js") || t.endsWith(".jsx")) return "javascript";
  if (t.endsWith(".py")) return "python";
  if (t.endsWith(".md")) return "markdown";
  if (t.endsWith(".json")) return "json";
  if (t.endsWith(".toml")) return "toml";
  if (t.endsWith(".yml") || t.endsWith(".yaml")) return "yaml";
  if (t.endsWith(".sql")) return "sql";
  if (t.endsWith(".sh")) return "bash";
  if (tool === "bash_executor") return "bash";
  return null;
}

function titleFor(msg: ChatMessageView, kind: string): string {
  // Prefer the first non-empty line, trimmed to 80 chars.
  const firstLine = (msg.content || "").split("\n").find((l) => l.trim());
  if (firstLine && firstLine.length <= 80) return firstLine.trim();
  if (firstLine) return `${firstLine.slice(0, 77).trim()}…`;
  return `${kind} · ${msg.id.slice(0, 8)}`;
}

function classifyKind(tool: string | null | undefined): string {
  if (!tool) return "other";
  if (tool === "bash_executor") return "bash_output";
  if (
    tool === "file_write" ||
    tool === "write_file" ||
    tool === "save_file" ||
    tool === "edit_file"
  ) {
    return "file";
  }
  if (tool === "notebook" || tool === "notebook_edit") return "code";
  return "other";
}

/** Does this message look like an artifact? */
function shouldCapture(msg: ChatMessageView): boolean {
  if (msg.role !== "tool") return false;
  if (!msg.content) return false;
  const tool = msg.tool_name ?? null;
  if (tool && ARTIFACT_TOOLS.has(tool)) return true;
  const lineCount = msg.content.split("\n").length;
  return lineCount >= MIN_LINES;
}

/**
 * Scan a session's message list and persist any newly-seen artifact-worthy
 * tool output. Idempotent — relies on the backend `save_artifact` dedup by
 * `source_message_id` so repeated calls do not create duplicates.
 *
 * `existing` is the current in-memory artifact list used to skip round-trips
 * for already-persisted messages.
 */
export async function detectAndPersist(
  sessionId: string,
  messages: ChatMessageView[],
  existing: Artifact[],
): Promise<void> {
  const known = new Set(
    existing.map((a) => a.source_message_id).filter((x): x is string => !!x),
  );

  for (const msg of messages) {
    if (!shouldCapture(msg)) continue;
    if (known.has(msg.id)) continue;
    const kind = classifyKind(msg.tool_name);
    const title = titleFor(msg, kind);
    const language = guessLanguage(msg.tool_name, title);
    try {
      await saveArtifact({
        session_id: sessionId,
        source_message_id: msg.id,
        kind,
        language,
        source_tool: msg.tool_name ?? null,
        title,
        content: msg.content,
      });
      known.add(msg.id);
    } catch (err) {
      // Non-fatal — artifact detection runs in the background and should not
      // block the chat. Log for debugging and move on.
      console.warn("[artifactDetect] save failed", err);
    }
  }
}
