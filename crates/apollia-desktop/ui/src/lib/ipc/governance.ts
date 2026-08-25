/**
 * Typed Tauri IPC wrappers for the tool-governance domain.
 *
 * Mirrors the `#[tauri::command]` functions in
 * `crates/apollia-desktop/src/commands/tool_governance.rs`. Components call
 * these instead of `invoke()` directly.
 */
import { invoke } from "@tauri-apps/api/core";

/**
 * Metadata of a credential stored for a tool.
 *
 * Deliberately carries no secret material: the backend `CredentialEntryDto`
 * exposes the key name and the timestamps only, and `narrowCredential` below
 * enforces that invariant a second time on the frontend side.
 *
 * The backend also carries a `last_used_at` timestamp. It is dropped on the way
 * in: the only writer of that column is the manual "test this key" action, on a
 * single hardcoded pair, so the value describes a manual test and never a real
 * use by an agent. Surfacing it would state a falsehood about every credential.
 */
export interface CredentialEntry {
  tool_name: string;
  key_name: string;
  /** ISO 8601 creation date. */
  created_at: string;
}

/**
 * Rebuilds a credential entry from the wire payload, keeping the three
 * non-secret fields and dropping everything else.
 *
 * A defensive projection: should the backend ever start returning a cleartext
 * value, it dies here instead of reaching a component.
 */
function narrowCredential(raw: CredentialEntry): CredentialEntry {
  return {
    tool_name: raw.tool_name,
    key_name: raw.key_name,
    created_at: raw.created_at,
  };
}

/**
 * Lists the configured credentials, optionally restricted to one tool.
 *
 * Values are never part of the response.
 */
export async function governanceListCredentials(
  toolName?: string,
): Promise<CredentialEntry[]> {
  const rows = await invoke<CredentialEntry[]>("governance_list_credentials", {
    toolName: toolName ?? null,
  });
  return rows.map(narrowCredential);
}
