/**
 * Allowlist-guarded IPC dispatch for the Next Steps cards.
 *
 * The cards are Meta-LLM output, so the command a card asks to run is data,
 * not code. The allowlist below is the last line of defence: only a command
 * it names reaches `invoke`, whatever the model generated. An entry added
 * here must have a matching `generate_handler!` registration; `memory_insert`
 * and `export_session` were once listed without one and failed silently.
 */
import { invoke } from "@tauri-apps/api/core";

const COMMAND_ALLOWLIST = new Set(["create_trigger", "install_agent"]);

/**
 * Run a card's command when the allowlist names it.
 *
 * Resolves `true` when the command ran, `false` when it was dropped as
 * non-allowlisted; the caller decides how to surface the drop.
 */
export async function invokeNextStepCommand(
  command: string,
  args: Record<string, unknown>,
): Promise<boolean> {
  if (!COMMAND_ALLOWLIST.has(command)) {
    return false;
  }
  await invoke(command, args);
  return true;
}
