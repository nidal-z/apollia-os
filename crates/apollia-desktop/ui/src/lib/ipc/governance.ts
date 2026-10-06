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

/**
 * Why governance fails closed, as `GET /api/v1/governance/state` answers it.
 *
 * `store_unreadable`: the permissions database cannot be read.
 * `policy_invalid`: an organisation policy is deposited and refused; `reason`
 * is its class (`schema`, `signature`, `key` or `owner`).
 * `journal_unavailable`: the organisation policy requires the journal, and the
 * journal is absent or could not be opened.
 */
export type FailClosed =
  | { cause: "store_unreadable"; detail: string }
  | { cause: "policy_invalid"; reason: string; detail: string }
  | { cause: "journal_unavailable"; detail: string };

/**
 * What degrades the journal without anything being refused.
 *
 * `unsigned`: the journal opened without its signing key; its entries are
 * chained, so an alteration still shows, and they are not signed.
 * `daily_export_failed`: the daily export the organisation policy asks for
 * could not be written to `directory`.
 */
export type JournalNotice =
  | { notice: "unsigned" }
  | { notice: "daily_export_failed"; directory: string; detail: string };

/**
 * The state of governance: `fail_closed` is `null` while it decides normally,
 * `journal_notices` is empty while the journal is in order.
 */
export interface GovernanceState {
  fail_closed: FailClosed | null;
  journal_notices: JournalNotice[];
}

/**
 * Reads the sources of governance again and says whether it fails closed.
 * While it does, every sensitive action and every new connection is refused;
 * reading continues.
 */
export async function governanceState(): Promise<GovernanceState> {
  const raw = await invoke<GovernanceState>("governance_state");
  return {
    fail_closed: raw.fail_closed ?? null,
    journal_notices: raw.journal_notices ?? [],
  };
}
