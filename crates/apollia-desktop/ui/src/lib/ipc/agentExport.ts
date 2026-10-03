/**
 * Typed Tauri IPC wrapper behind the export of an agent from its sheet.
 *
 * Thin wrapper over `export_agent` in
 * `crates/apollia-desktop/src/commands/agent_export.rs`: the runtime builds the
 * `.apo` archive, signs it on request with the author key of its secret
 * store, and the shell writes it to the file the user chose.
 */
import { invoke } from "@tauri-apps/api/core";

/** What an export did, as the runtime reported it. */
export interface AgentExportReport {
  /** The file written. */
  path: string;
  /** File name the runtime offered: `<id>-<version>.apo`. */
  file_name?: string;
  /** Publisher written in the package. */
  publisher: string;
  /** Whether the package is signed. */
  signed: boolean;
  /** Fingerprint of the key that signed, when signed. */
  key_fingerprint: string | null;
  /** Accesses left out: those into another package, agent or workflow. */
  removed_access: string[];
  /** Connection types the package requires of whoever installs it. */
  required_connections: string[];
}

/**
 * Exports the agent `name` to `path`, signed when `sign`, under `publisher`
 * (the `author.name` setting when `null`).
 */
export async function exportAgent(
  name: string,
  sign: boolean,
  publisher: string | null,
  path: string,
): Promise<AgentExportReport> {
  return invoke<AgentExportReport>("export_agent", { name, sign, publisher, path });
}
