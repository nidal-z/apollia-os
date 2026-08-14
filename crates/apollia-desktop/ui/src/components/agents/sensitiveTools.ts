/**
 * Predicate behind the "asks for confirmation on each call" badge shown in
 * the agent Tools tab.
 *
 * The native list mirrors `WRAPPED_NATIVES` in
 * `crates/apollia-runtime/src/chat/manager/dispatcher.rs`: the six built-in
 * tools the runtime wraps behind a human confirmation. The two lists have no
 * shared source yet; keep them aligned by hand when the dispatcher set moves.
 */
export const HITL_CONFIRMED_NATIVE_TOOLS: readonly string[] = [
  "bash_executor",
  "python_executor",
  "file_write",
  "file_edit",
  "notebook_edit",
  "http_fetch",
];

export function isSensitiveTool(id: string): boolean {
  return (
    HITL_CONFIRMED_NATIVE_TOOLS.includes(id) ||
    id.includes("send") ||
    id.includes("delete")
  );
}
