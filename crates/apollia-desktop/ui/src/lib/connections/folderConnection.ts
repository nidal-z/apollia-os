/**
 * Adding a folder of this machine as a connection.
 *
 * Choosing "Folder" in the selection opens the system picker; the folder the
 * person picks is handed to the runtime, which admits, stores and journals
 * it. Held here, outside the component, so a test plays it against a fake
 * picker and fake commands. A refusal of the admission is told apart from a
 * failure by the reason the runtime gives, as for any other connection.
 */
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { addFolderConnection } from "$lib/ipc/connections";
import { refusalOf, type Refusal } from "$lib/integrations/addConnection";
import type { Connection } from "$lib/types";

export type FolderOutcome =
  | { kind: "cancelled" }
  | { kind: "added"; connection: Connection }
  | { kind: "refused"; refusal: Refusal }
  | { kind: "failed"; message: string };

export interface FolderDeps {
  /** Open the system folder picker; `null` when the person cancels. */
  pick?: (title: string) => Promise<string | null>;
  /** Add the folder at a path; the Tauri command by default. */
  add?: (path: string) => Promise<Connection>;
}

/** The system folder picker, one folder at a time. */
export async function pickFolder(title: string): Promise<string | null> {
  const picked = await openDialog({ directory: true, multiple: false, title });
  return typeof picked === "string" ? picked : null;
}

function messageOf(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "string") return err;
  return String(err);
}

/** Open the picker, then add the folder picked. */
export async function chooseAndAddFolder(
  pickerTitle: string,
  deps: FolderDeps = {},
): Promise<FolderOutcome> {
  const pick = deps.pick ?? pickFolder;
  const add = deps.add ?? addFolderConnection;
  let path: string | null;
  try {
    path = await pick(pickerTitle);
  } catch (err: unknown) {
    return { kind: "failed", message: messageOf(err) };
  }
  if (path === null) return { kind: "cancelled" };
  try {
    return { kind: "added", connection: await add(path) };
  } catch (err: unknown) {
    const message = messageOf(err);
    const refusal = refusalOf(message);
    return refusal ? { kind: "refused", refusal } : { kind: "failed", message };
  }
}

/** What the agents may do in a folder, by the families of its tools. */
export interface FolderAbilities {
  read: boolean;
  writeOrDelete: boolean;
}

export function folderAbilities(connection: Connection): FolderAbilities {
  return {
    read: connection.tools.some((t) => t.family === "read"),
    writeOrDelete: connection.tools.some((t) => t.family === "write_or_delete"),
  };
}

/**
 * The last two folders of `path`, joined by a chevron: "Documents › Clients".
 * A root or a single folder is shown as it is.
 */
export function readablePath(path: string): string {
  const parts = path.split(/[\\/]+/).filter((p) => p.length > 0);
  if (parts.length === 0) return path;
  return parts.slice(-2).join(" › ");
}
