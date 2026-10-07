/**
 * Typed Tauri command wrappers for the tokens of the local server: their
 * list, the creation of one and its revocation
 * (`crates/apollia-desktop/src/commands/server_tokens.rs`).
 */
import { invoke } from "@tauri-apps/api/core";

/** A token as the runtime lists it: never its value. */
export interface ServerToken {
  /** Identifier, which revokes it. */
  id: number;
  /** Name given at creation. */
  name: string;
  /** The last four characters of the value. */
  lastFour: string;
  /** When it was created. */
  createdAt: string;
  /** When it last opened the server, `null` if never. */
  lastUsedAt: string | null;
  /** Whether it was revoked. */
  revoked: boolean;
}

/** A token that was just created: its row and, this once, its value. */
export interface CreatedServerToken extends ServerToken {
  /** The value. No later call answers it again. */
  value: string;
}

/** What `createServerToken` rejects with. */
export interface ServerTokenRefusal {
  /** A name the runtime refuses, a name a token in use carries, or the rest. */
  kind: "name_invalid" | "name_taken" | "other";
  /** The runtime's own words. */
  message: string;
}

/** Lists the tokens of the local server, in use and revoked, oldest first. */
export function listServerTokens(): Promise<ServerToken[]> {
  return invoke<ServerToken[]>("list_server_tokens");
}

/**
 * Creates a named token and answers its value, the only time it is given.
 * Rejects with a `ServerTokenRefusal`.
 */
export function createServerToken(name: string): Promise<CreatedServerToken> {
  return invoke<CreatedServerToken>("create_server_token", { name });
}

/** Revokes a token by its identifier; answers the token, revoked. */
export function revokeServerToken(id: number): Promise<ServerToken> {
  return invoke<ServerToken>("revoke_server_token", { id });
}
