/**
 * Typed Tauri IPC wrappers behind the demonstration dataset of an example.
 *
 * An example the application proposes may come with a dataset to try it on: a
 * workbook to run a batch on and a small folder of reference documents. The
 * application carries it and copies it into a folder the user chooses, on
 * request and never before. Thin wrapper over
 * `crates/apollia-desktop/src/commands/example_dataset.rs`.
 */
import { invoke } from "@tauri-apps/api/core";
import { documentDir, join as pathJoin } from "@tauri-apps/api/path";

/** What a copy wrote, and where. */
export interface ExampleDatasetCopy {
  /** The folder the dataset was copied into. */
  destination: string;
  /** Absolute path of the workbook. */
  workbook: string;
  /** Absolute path of the folder of reference documents. */
  folder: string;
  /** How many rows the workbook holds under its header row. */
  rows: number;
  /** The files that were already there and were left as they are. */
  kept: string[];
}

/**
 * Copies the dataset of the example `example` into `destination`. A file
 * already there is left as it is; an example without a dataset rejects.
 */
export function copyExampleDataset(example: string, destination: string): Promise<ExampleDatasetCopy> {
  return invoke<ExampleDatasetCopy>("copy_example_dataset", { example, destination });
}

/**
 * The folder a copy is proposed in: `folderName` under the documents folder
 * of this user. `null` when this machine names no documents folder.
 */
export async function proposedDatasetFolder(folderName: string): Promise<string | null> {
  try {
    const proposed: unknown = await pathJoin(await documentDir(), folderName);
    return typeof proposed === "string" && proposed.length > 0 ? proposed : null;
  } catch {
    return null;
  }
}
