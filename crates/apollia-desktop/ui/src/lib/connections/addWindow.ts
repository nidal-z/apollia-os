/**
 * The window that adds a connection: one search over four groups, the card of
 * the chosen entry, and where a cancelled addition leads back to.
 *
 * Pure and DOM-free. The words an entry is searched by come from the caller,
 * which owns the translations.
 */
import type { ProviderId } from "$lib/connections/capabilities";
import { NATIVE_PROVIDERS } from "$lib/connections/connectionRows";
import { matchesCatalogueQuery, matchesWords } from "$lib/connections/catalogueSource";
import type { RegistryServerView } from "$lib/types";

/** The groups of the window, in the order they are shown. */
export type AddGroupKey = "apollia" | "computer" | "public" | "address";

export const ADD_GROUPS: readonly AddGroupKey[] = ["apollia", "computer", "public", "address"];

/** One entry of the window. */
export type AddEntry =
  | { key: string; kind: "native"; provider: ProviderId }
  | { key: "folder"; kind: "folder" }
  | { key: "mailbox"; kind: "mailbox" }
  | { key: string; kind: "catalogue"; entry: RegistryServerView; source: "selection" | "public" }
  | { key: "address"; kind: "address" };

export interface AddGroup {
  key: AddGroupKey;
  entries: AddEntry[];
}

/** The key of a catalogue entry in the window. */
export function catalogueKey(entry: RegistryServerView): string {
  return `entry:${entry.name}`;
}

export function nativeKey(provider: ProviderId): string {
  return `native:${provider}`;
}

export interface GroupInputs {
  selection: readonly RegistryServerView[];
  publicEntries: readonly RegistryServerView[];
  query: string;
  /** The words a built-in entry (native, folder, mailbox) is found by, in the interface language. */
  wordsOf: (key: string) => readonly string[];
  /** The name of a category of the selection, in the interface language. */
  categoryWords?: (category: string) => readonly string[];
}

/**
 * The groups the search leaves. Offered by Apollia: the native accounts, then
 * the selection; on this computer: a folder, a mailbox; the public catalogue,
 * once asked; and the address of a service, always last and never filtered,
 * since it is what remains when nothing else answers.
 */
export function addWindowGroups(inputs: GroupInputs): AddGroup[] {
  const { query, wordsOf, categoryWords = () => [] } = inputs;
  const builtIn = (key: string): boolean => matchesWords(wordsOf(key), query);
  const found = (entry: RegistryServerView): boolean => {
    if (matchesCatalogueQuery(entry, query)) return true;
    const category = entry.enrichment?.category ?? entry.category;
    return category ? matchesWords(categoryWords(category), query) : false;
  };
  const apollia: AddEntry[] = [
    ...NATIVE_PROVIDERS.map((provider): AddEntry => ({ key: nativeKey(provider), kind: "native", provider })),
  ].filter((e) => builtIn(e.key));
  for (const entry of inputs.selection) {
    if (found(entry)) {
      apollia.push({ key: catalogueKey(entry), kind: "catalogue", entry, source: "selection" });
    }
  }
  const computer: AddEntry[] = [
    { key: "folder", kind: "folder" } as const,
    { key: "mailbox", kind: "mailbox" } as const,
  ].filter((e) => builtIn(e.key));
  const publicGroup: AddEntry[] = inputs.publicEntries
    .filter(found)
    .map((entry) => ({ key: catalogueKey(entry), kind: "catalogue", entry, source: "public" }));
  return [
    { key: "apollia", entries: apollia },
    { key: "computer", entries: computer },
    { key: "public", entries: publicGroup },
    { key: "address", entries: [{ key: "address", kind: "address" }] },
  ];
}

/** The entry `key` names among the groups, or null when the search left it out. */
export function findEntry(groups: readonly AddGroup[], key: string | null): AddEntry | null {
  if (key === null) return null;
  for (const group of groups) {
    const found = group.entries.find((e) => e.key === key);
    if (found) return found;
  }
  return null;
}

/** What adding an entry asks of the person, as message keys under `connections.add_window.needs`. */
export type AddNeed =
  | "sign_in"
  | "folder"
  | "mailbox"
  | "key"
  | "values"
  | "download"
  | "online"
  | "address"
  | "nothing";

/** What adding `entry` will ask: a sign-in, a key, values, a download, the network. */
export function entryNeeds(entry: AddEntry): AddNeed[] {
  switch (entry.kind) {
    case "native":
      return ["sign_in", "online"];
    case "folder":
      return ["folder"];
    case "mailbox":
      return ["mailbox", "online"];
    case "address":
      return ["address", "online"];
    case "catalogue":
      return catalogueNeeds(entry.entry);
  }
}

function catalogueNeeds(entry: RegistryServerView): AddNeed[] {
  const pkg = entry.packages?.[0] ?? null;
  const remote = entry.remotes[0] ?? null;
  const fields = [
    ...(pkg?.environmentVariables ?? []),
    ...(remote?.headers ?? []),
  ].filter((f) => f.isRequired);
  const needs: AddNeed[] = [];
  if (fields.some((f) => f.isSecret)) needs.push("key");
  const askedArgs = (pkg?.packageArguments ?? []).some((a) => a.isRequired && a.value === null);
  if (fields.some((f) => !f.isSecret) || askedArgs) needs.push("values");
  if (remote === null && pkg !== null) needs.push("download");
  if (remote !== null) needs.push("online");
  return needs.length > 0 ? needs : ["nothing"];
}

/**
 * Where the window stands. `adding` is set once Add was pressed on the card:
 * the flow of the addition stands in the card's place. `resume` names the
 * connection a sign-in done in its own dialog added, for the flow to resume on.
 * `away` is set while a dialog the flow opened (the sign-in to an account)
 * stands in the window's place, so that cancelling it brings the window back
 * on its search.
 */
export interface AddWindowState {
  open: boolean;
  query: string;
  chosen: string | null;
  adding: boolean;
  resume: string | null;
  away: boolean;
  /** The address of a mailbox kept already, whose password the flow asks again. */
  mailbox: string;
}

export const ADD_WINDOW_CLOSED: AddWindowState = {
  open: false,
  query: "",
  chosen: null,
  adding: false,
  resume: null,
  away: false,
  mailbox: "",
};

/** Each opening starts on an empty search, nothing chosen. */
export function openAddWindow(): AddWindowState {
  return { ...ADD_WINDOW_CLOSED, open: true };
}

/** The window opened on the card of one entry, asked for from another screen. */
export function openOnEntry(key: string): AddWindowState {
  return { ...ADD_WINDOW_CLOSED, open: true, chosen: key };
}

/**
 * The repair of a mailbox whose password was refused: the flow of a mailbox,
 * its address filled in, so only the password is typed again.
 */
export function openOnMailbox(address: string): AddWindowState {
  return { ...ADD_WINDOW_CLOSED, open: true, chosen: "mailbox", adding: true, mailbox: address };
}

/**
 * Another entry chosen in the list: its card opens, never its flow, so an
 * entry is added only once its own card and warning were read.
 */
export function chooseEntry(state: AddWindowState, key: string): AddWindowState {
  return { ...state, chosen: key, adding: false, resume: null, mailbox: "" };
}

/** Cancel on a card or in a flow not yet added: the search, as it was typed. */
export function backToSearch(state: AddWindowState): AddWindowState {
  return { ...state, chosen: null, adding: false, resume: null, mailbox: "" };
}

/** Add on the card: the flow of the entry takes the card's place. */
export function startAdding(state: AddWindowState): AddWindowState {
  return { ...state, adding: true, resume: null };
}

/**
 * A dialog of the flow takes the place of the window. A flow started while
 * the window is closed (from another screen) is not the window's, and
 * cancelling it will not open it.
 */
export function leaveForFlow(state: AddWindowState): AddWindowState {
  return { ...state, open: false, away: state.open || state.away };
}

/**
 * The dialog was cancelled: back to the search it left, the words kept and no
 * card open. A dialog the window did not open changes nothing.
 */
export function flowCancelled(state: AddWindowState): AddWindowState {
  if (!state.away) return state;
  return { ...state, open: true, chosen: null, adding: false, resume: null, away: false, mailbox: "" };
}

/**
 * The dialog added `connectionId`: the window comes back on the flow it left,
 * which goes on from who may use it.
 */
export function flowResumed(state: AddWindowState, connectionId: string): AddWindowState {
  if (!state.away) return state;
  return { ...state, open: true, adding: true, resume: connectionId, away: false };
}

/** The flow is over: the window is done with. */
export function flowDone(): AddWindowState {
  return ADD_WINDOW_CLOSED;
}
