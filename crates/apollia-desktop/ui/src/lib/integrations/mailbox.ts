/**
 * Adding a mailbox from Connections: the address, an application password,
 * and one button that tries the connection for real before it is kept.
 *
 * The server and port are filled from the address, from the providers the
 * runtime knows, and stay editable under Advanced. An address on another
 * domain is given the usual `imap.<domain>` on the implicit TLS port, which
 * the person corrects under Advanced if the trial says the server cannot be
 * reached. A failure is told in plain words, by what the runtime answered.
 */
import { addImapMailbox, type ImapProviders, type MailboxAccount } from "$lib/ipc/connections";
import { refusalOf, type Refusal } from "$lib/integrations/addConnection";

/** Port of IMAP over implicit TLS, used until the providers are read. */
export const IMAP_TLS_PORT = 993;

/** The server and port filled from an address, and the provider when it is a known one. */
export interface MailboxPrefill {
  host: string;
  port: number;
  provider: string | null;
}

/** Domain of an address, lowercase, or `null` when the text is not one yet. */
export function emailDomain(email: string): string | null {
  const trimmed = email.trim();
  const at = trimmed.lastIndexOf("@");
  if (at <= 0 || at === trimmed.length - 1) return null;
  const domain = trimmed.slice(at + 1).replace(/\.+$/, "").toLowerCase();
  if (domain === "" || /\s/.test(domain) || !domain.includes(".")) return null;
  return domain;
}

/** The server and port for `email`, or `null` while it is not an address. */
export function prefillFor(email: string, known: ImapProviders | null): MailboxPrefill | null {
  const domain = emailDomain(email);
  if (domain === null) return null;
  const provider = known?.providers.find((p) => p.domains.includes(domain));
  if (provider) return { host: provider.host, port: provider.port, provider: provider.name };
  return { host: `imap.${domain}`, port: known?.defaultPort ?? IMAP_TLS_PORT, provider: null };
}

/** What the person typed in the form. */
export interface MailboxForm {
  email: string;
  password: string;
  host: string;
  port: string;
}

/** The port typed under Advanced, or `null` when it is not one. */
export function parsePort(text: string): number | null {
  const trimmed = text.trim();
  if (!/^\d{1,5}$/.test(trimmed)) return null;
  const port = Number(trimmed);
  return port >= 1 && port <= 65535 ? port : null;
}

/** Whether the form holds enough to try: an address, a password, a server and a port. */
export function canAdd(form: MailboxForm): boolean {
  return (
    emailDomain(form.email) !== null &&
    form.password !== "" &&
    form.host.trim() !== "" &&
    parsePort(form.port) !== null
  );
}

/**
 * The message key, in plain words, for what the runtime answered. The texts
 * matched are the runtime's own messages for each failure of a mailbox.
 */
export function mailboxErrorKey(message: string): string {
  const lower = message.toLowerCase();
  if (lower.includes("refused the identifier or the password")) {
    return "connections.mailbox.error.credentials_rejected";
  }
  if (lower.includes("cannot be reached")) return "connections.mailbox.error.unreachable";
  if (lower.includes("secure connection")) return "connections.mailbox.error.tls";
  if (lower.includes("is not an email address")) return "connections.mailbox.error.invalid_email";
  if (lower.includes("no imap host is known") || lower.includes("is not a server address")) {
    return "connections.mailbox.error.invalid_host";
  }
  if (lower.includes("not a server port")) return "connections.mailbox.error.invalid_port";
  if (lower.includes("password is empty or too long")) {
    return "connections.mailbox.error.invalid_password";
  }
  if (lower.includes("outside the protocol")) return "connections.mailbox.error.protocol";
  if (lower.includes("secret store")) return "connections.mailbox.error.secret_store";
  if (
    lower.includes("failed to connect to runtime") ||
    lower.includes("not routed") ||
    lower.includes("request failed")
  ) {
    return "connections.mailbox.error.runtime";
  }
  return "connections.mailbox.error.generic";
}

export type MailboxOutcome =
  | { kind: "added"; account: MailboxAccount }
  | { kind: "refused"; refusal: Refusal }
  | { kind: "failed"; messageKey: string; detail: string };

function messageOf(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "string") return err;
  return String(err);
}

/**
 * Try the mailbox, then keep it: one call, since the runtime stores a mailbox
 * only once its login succeeded. A refusal of the admission is told apart
 * from a failure of the trial.
 */
export async function addMailbox(form: MailboxForm): Promise<MailboxOutcome> {
  try {
    const account = await addImapMailbox({
      email: form.email.trim(),
      host: form.host.trim() === "" ? null : form.host.trim(),
      port: parsePort(form.port),
      password: form.password,
    });
    return { kind: "added", account };
  } catch (err: unknown) {
    const detail = messageOf(err);
    const refusal = refusalOf(detail);
    if (refusal) return { kind: "refused", refusal };
    return { kind: "failed", messageKey: mailboxErrorKey(detail), detail };
  }
}
