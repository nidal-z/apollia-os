/**
 * Typed Tauri IPC wrappers behind the preview of a `.apo` package.
 *
 * Thin wrappers over `crates/apollia-desktop/src/commands/package_files.rs`:
 * the runtime reads the archive at its path on this machine, says what it
 * contains and asks for, installs or updates it with the confirmations the
 * user gave, and trusts the key that signed it. The shapes mirror the
 * `PackagePreview` schema of `POST /api/v1/packages/preview`.
 */
import { invoke } from "@tauri-apps/api/core";

/** State of the signature of a package. */
export type SignatureStatus = "unsigned" | "signed" | "invalid";

/** The publisher and the signature. */
export interface SignaturePreview {
  status: SignatureStatus;
  /** Name shown: the keyring label when trusted, else the declared name. */
  publisher: string;
  /** Name the package declares, not verified. */
  declaredPublisher: string;
  /** SHA-256 fingerprint of the signing key, when the signature holds. */
  keyFingerprint?: string | null;
  /** Signing public key, when the signature holds: what trusting imports. */
  publicKey?: string | null;
  /** The signing key is in the keyring. */
  trusted: boolean;
  /** Why the signature does not hold. */
  reason?: string | null;
}

/** One access an agent of the package asks for. */
export interface AccessPreview {
  accessId: string;
  permits: string;
  sensitive: boolean;
  /** `read`, `read_personal`, `write_or_delete`, `send_external`, `execute_code` or `unclassified`. */
  family: string;
}

export interface AgentPreview {
  id: string;
  name: string;
  mission: string;
  examples: string[];
  access: AccessPreview[];
}

export interface WorkflowPreview {
  name: string;
  description: string;
  family: string;
  sensitive: boolean;
  steps: string[];
}

export interface ToolPreview {
  name: string;
  description: string;
  family: string;
  sensitive: boolean;
}

export interface RequiredConnectionPreview {
  type: string;
  reason: string;
  present: boolean;
  broughtByPackage: boolean;
}

export interface BroughtServerPreview {
  name: string;
  transport: string;
  command: string;
  url?: string | null;
  remote: boolean;
  alreadyPresent: boolean;
}

export interface AddedAccess {
  agentId: string;
  accessId: string;
  permits: string;
  sensitive: boolean;
  family: string;
}

export interface UpdateDiff {
  fromVersion: string;
  toVersion: string;
  downgrade: boolean;
  addedAccess: AddedAccess[];
  addedRequires: string[];
  addedMcpServers: string[];
}

/** Why the package cannot be installed as it is, tagged by `code`. */
export type InstallRefusal =
  | { code: "invalid_signature"; reason: string }
  | { code: "signature_required" }
  | { code: "publisher_not_trusted"; fingerprint: string }
  | { code: "downgrade"; installed: string; incoming: string }
  | { code: "agent_taken"; agent: string; owner?: string | null };

/** Everything the user sees before installing a package. */
export interface PackagePreview {
  name: string;
  version: string;
  description: string;
  sdk: string;
  signature: SignaturePreview;
  signatureRequired: boolean;
  agents: AgentPreview[];
  workflows: WorkflowPreview[];
  tools: ToolPreview[];
  requires: RequiredConnectionPreview[];
  mcpServers: BroughtServerPreview[];
  pip: string[];
  triggers: { id: string; target: string; source: string }[];
  installedVersion?: string | null;
  update?: UpdateDiff | null;
  notices: { code: string }[];
  refusal?: InstallRefusal | null;
}

/** What an installation did, as the runtime reported it. */
export interface PackageInstallReport {
  name: string;
  version: string;
  [key: string]: unknown;
}

/** A trusted publisher key. */
export interface TrustedKeyView {
  fingerprint: string;
  publicKey: string;
  label: string;
  origin: string;
  addedAt: string;
}

/** What the package at `path` contains and asks for; nothing is written. */
export async function previewPackageFile(path: string): Promise<PackagePreview> {
  return invoke<PackagePreview>("preview_package_file", { path });
}

/**
 * Installs, or updates, the package at `path`, with the Python requirements
 * accepted when `depsConfirmed` and what an update adds when `changesConfirmed`.
 */
export async function installPackageFile(
  path: string,
  depsConfirmed: boolean,
  changesConfirmed: boolean,
): Promise<PackageInstallReport> {
  return invoke<PackageInstallReport>("install_package_file", {
    path,
    depsConfirmed,
    changesConfirmed,
  });
}

/** Trusts the publisher key `publicKey`, shown as `label`. */
export async function trustPackagePublisher(
  publicKey: string,
  label: string,
): Promise<TrustedKeyView> {
  return invoke<TrustedKeyView>("trust_package_publisher", { publicKey, label });
}
