/**
 * Typed Tauri IPC wrapper for the organisation policy screen.
 *
 * The screen only reads: the policy is written and signed by IT on the
 * command line, so this module exposes no command that changes it.
 */
import { invoke } from "@tauri-apps/api/core";

/** A rule that can only refuse. */
export type DenyOnly = "deny";

/** An action family a policy can refuse. */
export type PolicyFamily =
  | "read"
  | "read_personal"
  | "write_or_delete"
  | "send_external"
  | "execute_code";

/** The sections of `policy.toml`, as the runtime serves them. */
export interface OrgPolicyBody {
  schema_version: number;
  signed_at: string;
  connections: { allow: string[]; deny: string[]; remote: DenyOnly | null };
  families: { deny: PolicyFamily[] };
  inference: {
    allowed_servers: string[];
    remote_endpoints: DenyOnly | null;
    require_model_hash: boolean;
  };
  settings: Record<string, unknown>;
  packages: { require_signature: boolean; trusted_publishers: string[] };
  journal: {
    required: boolean;
    retention_days: number | null;
    export: { detail: "forbid" | "require" | null; daily_summary_dir: string | null };
  };
}

/** Why a present policy is refused. */
export type PolicyInvalidReason = "schema" | "signature" | "key" | "owner";

/**
 * The policy the runtime read at startup, in the form
 * `apollia-os policy show --json` prints.
 */
export type PolicyView =
  | { state: "none"; dir: string }
  | { state: "invalid"; dir: string; reason: PolicyInvalidReason; detail: string }
  | {
      state: "active";
      dir: string;
      key_fingerprint: string;
      publisher: string;
      signed_at: string;
      policy: OrgPolicyBody;
    };

/** The organisation policy the runtime applies. */
export async function getOrgPolicy(): Promise<PolicyView> {
  return invoke<PolicyView>("get_org_policy");
}
