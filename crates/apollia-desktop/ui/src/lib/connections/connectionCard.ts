/**
 * The decisions of the card of one connection: the one action that repairs
 * it, which of pause and resume it offers, what its removal will erase, said
 * in words, and the connection found by a gesture put back in the list.
 *
 * Pure and i18n-agnostic: it returns keys and values, the component resolves
 * them with the live formatter.
 */
import type { I18nDescriptor } from "$lib/connections/status";
import type { ConnectionRemoval, RemovedItem } from "$lib/ipc/connections";
import type { Connection } from "$lib/types";

/**
 * What repairs a connection: sign in again (a native account, or a remote
 * service that asks for it), enter the password again (a mailbox), choose the folder
 * again (a folder), see the rule that blocks it, start a service again, or
 * send the administrator's approval and connect again (an organisation that
 * requires it).
 */
export type RepairKind = "sign_in" | "password" | "folder" | "rule" | "restart" | "admin_approval";

/** The one action that repairs a connection, and the key of its word. */
export interface RepairAction {
  kind: RepairKind;
  labelKey: string;
}

const REPAIR_LABEL: Record<RepairKind, string> = {
  sign_in: "connections.card.repair.sign_in",
  password: "connections.card.repair.password",
  folder: "connections.card.repair.folder",
  rule: "connections.card.repair.rule",
  restart: "connections.card.repair.restart",
  admin_approval: "common.retry",
};

function action(kind: RepairKind): RepairAction {
  return { kind, labelKey: REPAIR_LABEL[kind] };
}

/**
 * The one action that repairs `connection`, or null when it works, is paused,
 * was never checked, or when checking it again is all there is to do.
 */
export function repairAction(
  connection: Pick<Connection, "kind" | "status" | "reason" | "remote">,
): RepairAction | null {
  // An organisation that requires its administrator's approval is not a
  // rule of Apollia: what repairs it is the approval page kept with it.
  if (connection.kind === "native" && connection.reason?.key === "admin_approval_required" && connection.reason.address) {
    return action("admin_approval");
  }
  if (connection.status === "blocked") return action("rule");
  if (connection.status !== "needs_repair") return null;
  switch (connection.reason?.key) {
    case "forbidden_by_policy":
      return action("rule");
    case "password_refused":
      return connection.kind === "imap" ? action("password") : null;
    case "folder_not_found":
      return connection.kind === "folder" ? action("folder") : null;
    case "sign_in_again":
    case "admin_approval_required":
      if (connection.kind === "native") return action("sign_in");
      // A remote service signs in through its own OAuth; a local one has no
      // sign-in, so starting it again is what is left.
      if (connection.kind === "mcp") return action(connection.remote ? "sign_in" : "restart");
      return null;
    case "unreachable":
    case "failing":
      return connection.kind === "mcp" ? action("restart") : null;
    default:
      return null;
  }
}

/** Which of pause and resume the card offers; a blocked connection offers neither. */
export function pauseGesture(status: Connection["status"]): "pause" | "resume" | null {
  if (status === "blocked") return null;
  return status === "paused" ? "resume" : "pause";
}

const REMOVED_KEY: Record<Exclude<RemovedItem["kind"], "state">, string> = {
  configuration: "connections.card.remove.item.configuration",
  installed_files: "connections.card.remove.item.installed_files",
  token: "connections.card.remove.item.token",
  secret: "connections.card.remove.item.secret",
  agent_access: "connections.card.remove.item.agent_access",
  source: "connections.card.remove.item.source",
  onedrive_choice: "connections.card.remove.item.onedrive_choice",
};

/**
 * What a removal will erase, one line per thing, in the order the runtime
 * gives. The state kept for the connection is not a thing its user knows of,
 * so it is not listed.
 */
export function removalLines(items: readonly RemovedItem[]): I18nDescriptor[] {
  const lines: I18nDescriptor[] = [];
  for (const item of items) {
    if (item.kind === "state") continue;
    const values: Record<string, string> = { name: item.name };
    if (item.agent) values.agent = item.agent;
    lines.push({ key: REMOVED_KEY[item.kind], values });
  }
  return lines;
}

/** Whether a removal left something it could not erase. */
export function removalLeftSomething(removal: ConnectionRemoval): boolean {
  return removal.left.length > 0;
}

/** `connections` with `updated` in the place of the connection it replaces. */
export function replaceConnection(connections: readonly Connection[], updated: Connection): Connection[] {
  return connections.map((c) => (c.id === updated.id ? updated : c));
}

/**
 * Check the connection `id` with `check`, then put the connection found in
 * the place of the one `current` gives at that moment: the list is not read
 * again, and what changed in it meanwhile is kept.
 */
export async function checkAndReplace(
  id: string,
  check: (id: string) => Promise<Connection>,
  current: () => readonly Connection[],
): Promise<Connection[]> {
  const found = await check(id);
  return replaceConnection(current(), found);
}
