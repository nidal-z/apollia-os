// Typed IPC wrapper for the inter-agent mailbox observability surface.
//
// `listMailboxMessages` reads the durable `mailbox.db` store written by the
// runtime mailbox actor (ctx.mail) and returns the most recent messages across
// every recipient, newest first. Keeping the call here removes direct `invoke`
// usage from the `.svelte` file.

import { invoke } from "@tauri-apps/api/core";

/** One inter-agent mailbox message, mirroring the Rust `MailboxMessageRow`. */
export interface MailboxMessageRow {
  /** Stable message identifier (primary key). */
  message_id: string;
  /** Sending agent name (or `host:<id>` for a host injection). */
  from_agent: string;
  /** Receiving agent name. */
  to_agent: string;
  /** Raw JSON payload as stored, rendered verbatim in the UI. */
  payload: string;
  /** Send timestamp (RFC 3339). */
  sent_at: string;
  /** Delivery state: `pending` or `in_flight`. */
  state: string;
}

/**
 * Lists the most recent mailbox messages across every recipient.
 *
 * @param limit maximum rows to return (default 100 on the Rust side).
 */
export async function listMailboxMessages(limit = 100): Promise<MailboxMessageRow[]> {
  return invoke<MailboxMessageRow[]>("list_mailbox_messages", { limit });
}
