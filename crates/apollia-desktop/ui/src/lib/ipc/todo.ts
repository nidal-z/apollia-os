// Typed IPC wrappers for the live session todo panel.
//
// The runtime owns the todo list (one actor per session) and pushes a full
// snapshot through the dedicated `"todo-updated"` Tauri event after every
// change. The initial state is read once through the `get_session_todo` command.

import { invoke } from "@tauri-apps/api/core";

/** Lifecycle status of a todo item, mirrors the runtime `TodoStatus`. */
export type TodoStatus = "pending" | "in_progress" | "completed";

/**
 * A single todo item as serialized by the runtime.
 *
 * Mirrors `apollia_core::todo::TodoItem`: the wire payload carries `id`,
 * `content`, `status` and `depends_on` only. There is no `created_at` or
 * `updated_at` field (those are storage columns, not part of the snapshot).
 */
export interface TodoItem {
  id: string;
  content: string;
  status: TodoStatus;
  depends_on: string[];
}

/** Payload of the `"todo-updated"` Tauri event (one full snapshot per change). */
export interface TodoUpdatedPayload {
  session_id: string;
  items: TodoItem[];
}

/** Reads the current todo list for a session (one-shot initial load). */
export async function getSessionTodo(sessionId: string): Promise<TodoItem[]> {
  return invoke<TodoItem[]>("get_session_todo", { sessionId });
}
