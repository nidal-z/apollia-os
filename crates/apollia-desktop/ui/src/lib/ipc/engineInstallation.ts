// Typed IPC wrappers for the followed installation of an engine: the
// accelerated engine the AI screen proposes, its import from a folder, and
// its updates.
//
// Every call answers the state of the installation as the runtime reports
// it. A refusal is a state with its code, never a rejected promise.

import { invoke } from "@tauri-apps/api/core";

/** What an installation is: a first download, a newer build, or an import from a folder. */
export type EngineInstallationKind = "install" | "update" | "import";

/** Why the engine store refused, as the runtime names it. */
export type EngineRefusalCode =
  | "no_trusted_key"
  | "signature_invalid"
  | "signature_unknown_key"
  | "signature_missing"
  | "manifest_invalid"
  | "rollback"
  | "package_not_found"
  | "digest_mismatch"
  | "archive_invalid"
  | "network"
  | "storage";

/** Where the installation of an engine stands. */
export type EngineInstallation =
  | { state: "idle" }
  /** The signature is being verified; no archive was requested. */
  | { state: "verifying"; kind: EngineInstallationKind }
  /** The signature held and the archive is arriving, in bytes. */
  | { state: "downloading"; kind: EngineInstallationKind; received: number; total: number }
  /** The signature held and the archives of the folder are being installed. */
  | { state: "installing"; kind: EngineInstallationKind }
  | { state: "installed"; kind: EngineInstallationKind }
  | {
      state: "failed";
      kind: EngineInstallationKind;
      code: EngineRefusalCode;
      /** The runtime's sentence. */
      error: string;
      /** False when the refusal came before any archive was requested. */
      archive_requested: boolean;
    };

/** Where the installation stands. */
export async function getEngineInstallation(): Promise<EngineInstallation> {
  return invoke<EngineInstallation>("get_engine_installation");
}

/**
 * Start the verified download of the newest build of one engine variant. The
 * signature of the published list is verified before any archive is asked for.
 */
export async function startEngineInstallation(
  engine: string,
  variant: string,
): Promise<EngineInstallation> {
  return invoke<EngineInstallation>("start_engine_installation", { engine, variant });
}

/** Start the import of the engines of a folder: archive, list and signature. */
export async function importEngineFolder(path: string): Promise<EngineInstallation> {
  return invoke<EngineInstallation>("import_engine_folder", { path });
}

/** Cancel the download, or forget the outcome of the last installation. */
export async function dismissEngineInstallation(): Promise<EngineInstallation> {
  return invoke<EngineInstallation>("dismiss_engine_installation");
}
