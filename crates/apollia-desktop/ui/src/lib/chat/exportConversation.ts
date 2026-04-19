/**
 * Conversation export — formats a chat session as Markdown, JSON, or
 * Markdown-with-tools (US-SP42-026, B.43).
 *
 * Pure functions, no Tauri/IPC coupling — the backend command
 * `export_conversation` reuses the same logic server-side, but the UI
 * may also render a preview locally without a round-trip.
 */
import type { ChatMessageView, ChatSessionDetail, ToolCallView } from "$lib/types";

export type ExportFormat = "markdown" | "json" | "markdown-with-tools";

export function exportConversation(
  session: ChatSessionDetail,
  format: ExportFormat,
): { content: string; filename: string; mime: string } {
  const stamp = isoSlug(session.created_at);
  const slug = slugify(session.title ?? session.agent_name ?? session.mode);
  const base = `apollia-chat-${slug}-${stamp}`;
  switch (format) {
    case "json":
      return {
        content: JSON.stringify(session, null, 2),
        filename: `${base}.json`,
        mime: "application/json",
      };
    case "markdown":
      return {
        content: toMarkdown(session, false),
        filename: `${base}.md`,
        mime: "text/markdown",
      };
    case "markdown-with-tools":
      return {
        content: toMarkdown(session, true),
        filename: `${base}.tools.md`,
        mime: "text/markdown",
      };
  }
}

function toMarkdown(session: ChatSessionDetail, withTools: boolean): string {
  const lines: string[] = [];
  const title = session.title ?? session.agent_name ?? "Chat conversation";
  lines.push(`# ${title}`);
  lines.push("");
  lines.push(`- **Session:** ${session.id}`);
  lines.push(`- **Mode:** ${session.mode}${session.agent_name ? ` (${session.agent_name})` : ""}`);
  lines.push(`- **Created:** ${session.created_at}`);
  if (session.closed_at) lines.push(`- **Closed:** ${session.closed_at}`);
  if (session.llm_backend) lines.push(`- **LLM:** ${session.llm_backend}`);
  lines.push("");
  lines.push("---");
  lines.push("");

  for (const msg of session.messages ?? []) {
    lines.push(...renderMessage(msg, withTools));
    lines.push("");
  }

  return lines.join("\n");
}

function renderMessage(msg: ChatMessageView, withTools: boolean): string[] {
  const out: string[] = [];
  const role = roleLabel(msg.role);
  out.push(`## ${role} — ${msg.created_at}`);
  if (msg.content?.trim()) {
    out.push("");
    out.push(msg.content.trim());
  }
  if (withTools && msg.tool_calls && msg.tool_calls.length > 0) {
    out.push("");
    for (const call of msg.tool_calls as ToolCallView[]) {
      out.push(`<details><summary>🔧 ${call.tool_name} · ${call.status}</summary>`);
      out.push("");
      out.push("**Input:**");
      out.push("```json");
      out.push(JSON.stringify(call.input, null, 2));
      out.push("```");
      if (call.output) {
        out.push("");
        out.push("**Output:**");
        out.push("```");
        out.push(call.output);
        out.push("```");
      }
      out.push("");
      out.push("</details>");
    }
  }
  if (withTools && msg.role === "tool" && msg.tool_name) {
    out.push("");
    out.push(`> Tool: \`${msg.tool_name}\``);
  }
  return out;
}

function roleLabel(role: string): string {
  switch (role) {
    case "user":
      return "🧑 User";
    case "assistant":
      return "🤖 Assistant";
    case "tool":
      return "🔧 Tool";
    case "system":
      return "⚙️ System";
    default:
      return role;
  }
}

function slugify(input: string): string {
  return input
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 40) || "chat";
}

function isoSlug(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "unknown";
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}`;
}
