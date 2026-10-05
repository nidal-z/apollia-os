/**
 * Typed Tauri IPC wrappers behind the example packages the Agents screen
 * proposes.
 *
 * Thin wrappers over `crates/apollia-desktop/src/commands/package_examples.rs`:
 * the runtime carries the examples, lists them with what each requires and
 * whether the user installed it, previews one and installs it. An example is
 * named by its package, never by a path. The shapes mirror the
 * `ExamplePackage` schema of `GET /api/v1/packages/examples`.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  PackageInstallReport,
  PackagePreview,
  RequiredConnectionPreview,
} from "$lib/ipc/packageFiles";

/** One agent of an example. */
export interface ExampleAgent {
  id: string;
  name: string;
  mission: string;
}

/** An example as the application proposes it. */
export interface ExamplePackage {
  /** Package identifier. */
  name: string;
  /** Version the application carries. */
  version: string;
  description: string;
  agents: ExampleAgent[];
  requires: RequiredConnectionPreview[];
  /** Installed version, when the user installed it. */
  installedVersion?: string | null;
}

/** The example packages the application proposes, installed or not. */
export async function listExamplePackages(): Promise<ExamplePackage[]> {
  return invoke<ExamplePackage[]>("list_example_packages");
}

/** What the example `name` contains and asks for; nothing is written. */
export async function previewExamplePackage(name: string): Promise<PackagePreview> {
  return invoke<PackagePreview>("preview_example_package", { name });
}

/**
 * Installs the example `name`, with the Python requirements accepted when
 * `depsConfirmed` and what an update adds when `changesConfirmed`.
 */
export async function installExamplePackage(
  name: string,
  depsConfirmed: boolean,
  changesConfirmed: boolean,
): Promise<PackageInstallReport> {
  return invoke<PackageInstallReport>("install_example_package", {
    name,
    depsConfirmed,
    changesConfirmed,
  });
}
