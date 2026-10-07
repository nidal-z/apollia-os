// The window "Manage tokens" of the local server: which tokens it lists, what
// creating one shows, what revoking one leaves, and what closing it forgets.
//
// The decisions live here, apart from the component, so Vitest can hold them
// without rendering anything. The value of a token is given once by the
// runtime, at its creation; it lives in the state of the open window and
// nowhere else. This module keeps nothing between two calls: every function
// takes the window and returns the next one.

import type { LocalServerView } from "$lib/ipc/localServer";
import type {
  CreatedServerToken,
  ServerToken,
  ServerTokenRefusal,
} from "$lib/ipc/serverTokens";
import type { ServerBarState } from "./localServer";

/** The i18n keys of the current. */
export const SERVER_TOKENS_TEXTS = {
  title: "dev.developer.tokens.title",
  intro: "dev.developer.tokens.intro",
  empty: "dev.developer.tokens.empty",
  createdOn: "dev.developer.tokens.created_on",
  lastUsed: "dev.developer.tokens.last_used",
  neverUsed: "dev.developer.tokens.never_used",
  nameLabel: "dev.developer.tokens.name_label",
  namePlaceholder: "dev.developer.tokens.name_placeholder",
  create: "dev.developer.tokens.create",
  createdTitle: "dev.developer.tokens.created_title",
  shownOnce: "dev.developer.tokens.shown_once",
  copy: "dev.developer.tokens.copy",
  copied: "dev.developer.tokens.copied",
  copyFailed: "dev.developer.tokens.copy_failed",
  revoke: "dev.developer.tokens.revoke",
  revokeLabel: "dev.developer.tokens.revoke_label",
  revokeTitle: "dev.developer.tokens.revoke_title",
  revokeMessage: "dev.developer.tokens.revoke_message",
  close: "dev.developer.tokens.close",
  nameEmpty: "dev.developer.tokens.name_empty",
  nameInvalid: "dev.developer.tokens.name_invalid",
  nameTaken: "dev.developer.tokens.name_taken",
  readFailed: "dev.developer.tokens.read_failed",
  createFailed: "dev.developer.tokens.create_failed",
  revokeFailed: "dev.developer.tokens.revoke_failed",
} as const;

/** The longest name the runtime accepts. */
export const LONGEST_TOKEN_NAME = 64;

/** Why the window could not do what was asked. */
export type TokensFailure =
  /** The list could not be read, in the runtime's words. */
  | { kind: "read"; message: string }
  /** Nothing was typed. */
  | { kind: "name_empty" }
  /** The name is too long or reads as an identifier. */
  | { kind: "name_invalid" }
  /** A token in use already carries the name. */
  | { kind: "name_taken"; name: string }
  /** The creation failed for another reason, in the runtime's words. */
  | { kind: "create"; message: string }
  /** The revocation failed, in the runtime's words. */
  | { kind: "revoke"; message: string };

/** The token that was just created, as long as the window stays open. */
export interface ShownToken {
  id: number;
  name: string;
  /** The value, shown once. */
  value: string;
}

/** The window as it stands. */
export interface TokensWindow {
  /** Every token the runtime knows, in use and revoked. */
  tokens: ServerToken[];
  /** Whether the list was read: a count is only told from a list that was. */
  read: boolean;
  /** The token created since the window opened, with its value. */
  shown: ShownToken | null;
  /** The failure to show, `null` when the last gesture went through. */
  failure: TokensFailure | null;
}

/** A window that holds nothing: before it opens, and after it closes. */
export const CLOSED_WINDOW: TokensWindow = {
  tokens: [],
  read: false,
  shown: null,
  failure: null,
};

/** What the window needs from the outside, replaceable in tests. */
export interface ServerTokensIpc {
  list: () => Promise<ServerToken[]>;
  /** Rejects with a `ServerTokenRefusal`. */
  create: (name: string) => Promise<CreatedServerToken>;
  revoke: (id: number) => Promise<ServerToken>;
}

/** The tokens the window lists: those that still open the server. */
export function tokensInUse(current: TokensWindow): ServerToken[] {
  return current.tokens.filter((token) => !token.revoked);
}

/**
 * The count "Active tokens: N" takes from the current, `null` while the list
 * was not read: an unread list says nothing of the count.
 */
export function activeTokenCount(current: TokensWindow): number | null {
  return current.read ? tokensInUse(current).length : null;
}

/** The bar with the count of tokens the window just told. */
export function withActiveTokens(bar: ServerBarState, count: number): ServerBarState {
  if (bar.view === null || bar.view.activeTokens === count) return bar;
  const view: LocalServerView = { ...bar.view, activeTokens: count };
  return { ...bar, view };
}

function words(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  if (typeof error === "object" && error !== null) {
    const message = (error as Record<string, unknown>).message;
    if (typeof message === "string") return message;
  }
  return String(error);
}

function refusalKind(error: unknown): ServerTokenRefusal["kind"] {
  if (typeof error !== "object" || error === null) return "other";
  const kind = (error as Record<string, unknown>).kind;
  return kind === "name_invalid" || kind === "name_taken" ? kind : "other";
}

/**
 * The name typed, trimmed; a failure for a name the runtime would refuse, so
 * nothing is sent for it.
 */
export function parseTokenName(text: string): string | TokensFailure {
  const name = text.trim();
  if (name === "") return { kind: "name_empty" };
  if ([...name].length > LONGEST_TOKEN_NAME || /^\d+$/.test(name)) return { kind: "name_invalid" };
  return name;
}

/** Opens the current: reads the list. A read that fails says why. */
export async function openTokens(ipc: ServerTokensIpc): Promise<TokensWindow> {
  try {
    return { tokens: await ipc.list(), read: true, shown: null, failure: null };
  } catch (error) {
    return { ...CLOSED_WINDOW, failure: { kind: "read", message: words(error) } };
  }
}

/**
 * Creates a token named as typed. The window then shows its value, and lists
 * the token under the others; a token shown before gives way to this one. A
 * refusal changes nothing but the failure.
 */
export async function createToken(
  ipc: ServerTokensIpc,
  current: TokensWindow,
  text: string,
): Promise<TokensWindow> {
  const name = parseTokenName(text);
  if (typeof name !== "string") return { ...current, failure: name };
  try {
    const { value, ...token } = await ipc.create(name);
    return {
      tokens: [...current.tokens, token],
      read: current.read,
      shown: { id: token.id, name: token.name, value },
      failure: null,
    };
  } catch (error) {
    const kind = refusalKind(error);
    if (kind === "name_invalid") return { ...current, failure: { kind } };
    if (kind === "name_taken") return { ...current, failure: { kind, name } };
    return { ...current, failure: { kind: "create", message: words(error) } };
  }
}

/**
 * Revokes a token: it leaves the tokens in use, and its value leaves the
 * window with it if it was the one shown.
 */
export async function revokeToken(
  ipc: ServerTokensIpc,
  current: TokensWindow,
  id: number,
): Promise<TokensWindow> {
  try {
    const revoked = await ipc.revoke(id);
    return {
      tokens: current.tokens.map((token) =>
        token.id === id ? { ...revoked, revoked: true } : token,
      ),
      read: current.read,
      shown: current.shown?.id === id ? null : current.shown,
      failure: null,
    };
  } catch (error) {
    return { ...current, failure: { kind: "revoke", message: words(error) } };
  }
}

/** Closes the current: nothing of it is kept, the value shown least of all. */
export function closeTokens(): TokensWindow {
  return CLOSED_WINDOW;
}

/** Copies the value shown; `false` when there is none or the copy fails. */
export async function copyShownToken(
  current: TokensWindow,
  write: (text: string) => Promise<void>,
): Promise<boolean> {
  if (current.shown === null) return false;
  try {
    await write(current.shown.value);
    return true;
  } catch {
    return false;
  }
}
