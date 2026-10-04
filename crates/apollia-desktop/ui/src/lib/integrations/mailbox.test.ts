import { beforeEach, describe, test, expect, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

import { invoke } from "@tauri-apps/api/core";

import {
  addMailbox,
  canAdd,
  emailDomain,
  mailboxErrorKey,
  parsePort,
  prefillFor,
  type MailboxForm,
} from "$lib/integrations/mailbox";
import { REMOTE_DENIED_REASON } from "$lib/integrations/addConnection";
import type { ImapProviders } from "$lib/ipc/connections";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";
import { readFileSync } from "node:fs";

interface Call {
  cmd: string;
  args: Record<string, unknown>;
}

/** Fake commands behind the Tauri bridge: each answer is a value, or a function that may throw. */
function fakeInvoke(
  calls: Call[],
  answers: Record<string, unknown | ((args: Record<string, unknown>) => unknown)>,
): void {
  vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
    const record = (args ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args: record });
    const answer = answers[cmd];
    return typeof answer === "function"
      ? (answer as (a: Record<string, unknown>) => unknown)(record)
      : answer;
  });
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

function lookup(catalogue: unknown, key: string): unknown {
  return key.split(".").reduce<unknown>(
    (node, part) => (node as Record<string, unknown> | undefined)?.[part],
    catalogue,
  );
}

/** The providers as `list_imap_providers` answers them. */
const PROVIDERS: ImapProviders = {
  providers: [
    { name: "Gmail", domains: ["gmail.com", "googlemail.com"], host: "imap.gmail.com", port: 993 },
    { name: "Orange", domains: ["orange.fr", "wanadoo.fr"], host: "imap.orange.fr", port: 993 },
  ],
  defaultPort: 993,
};

function form(over: Partial<MailboxForm> = {}): MailboxForm {
  return {
    email: "ana@gmail.com",
    password: "abcd efgh ijkl mnop",
    host: "imap.gmail.com",
    port: "993",
    ...over,
  };
}

describe("the mailbox form fills the server from the address", () => {
  test("a known provider gives its own server and port", () => {
    // GIVEN the providers the runtime knows
    // WHEN a Gmail address and an Orange address in capitals are typed
    const gmail = prefillFor("ana@gmail.com", PROVIDERS);
    const orange = prefillFor(" Ana@Wanadoo.FR ", PROVIDERS);

    // THEN each gets its provider's server and port
    expect(gmail).toEqual({ host: "imap.gmail.com", port: 993, provider: "Gmail" });
    expect(orange).toEqual({ host: "imap.orange.fr", port: 993, provider: "Orange" });
  });

  test("another domain gets the usual imap server, editable under Advanced", () => {
    // GIVEN the providers the runtime knows
    // WHEN an address on a domain outside the list is typed
    const prefill = prefillFor("claire@exemple.fr", PROVIDERS);

    // THEN the usual server of that domain is proposed, on the implicit TLS port
    expect(prefill).toEqual({ host: "imap.exemple.fr", port: 993, provider: null });
  });

  test("nothing is filled while the text is not yet an address", () => {
    // GIVEN texts typed on the way to an address
    // WHEN their server is asked for
    // THEN none is proposed
    for (const text of ["", "ana", "ana@", "@gmail.com", "ana@gmail"]) {
      expect(prefillFor(text, PROVIDERS)).toBeNull();
      expect(emailDomain(text)).toBeNull();
    }
  });

  test("the form explains the application password in one sentence, in both languages", () => {
    // GIVEN the two catalogues
    // WHEN the texts of the mailbox entry and form are read
    const keys = [
      "connections.mailbox.entry_name",
      "connections.mailbox.password_label",
      "connections.mailbox.password_hint",
      "connections.mailbox.advanced",
      "connections.mailbox.host_label",
      "connections.mailbox.port_label",
    ];

    // THEN each exists in both, the entry is named in French, and the explanation
    // is a single sentence that names the application password
    for (const key of keys) {
      expect(typeof lookup(fr, key), key).toBe("string");
      expect(typeof lookup(en, key), key).toBe("string");
    }
    expect(lookup(fr, "connections.mailbox.entry_name")).toBe("Bo\u00eete mail");
    const hint = lookup(fr, "connections.mailbox.password_hint") as string;
    expect(hint).toMatch(/mot de passe d'application/);
    expect(hint.trim().split(/[.!?](\s|$)/).filter((s) => s.trim() !== "")).toHaveLength(1);
  });
});

describe("the Add button is active only for a complete form", () => {
  test("an address, a password, a server and a port are needed", () => {
    // GIVEN forms missing one piece each
    // WHEN their readiness is read
    // THEN only the complete one can be added
    expect(canAdd(form())).toBe(true);
    expect(canAdd(form({ email: "ana" }))).toBe(false);
    expect(canAdd(form({ password: "" }))).toBe(false);
    expect(canAdd(form({ host: "  " }))).toBe(false);
    expect(canAdd(form({ port: "0" }))).toBe(false);
    expect(canAdd(form({ port: "99999" }))).toBe(false);
    expect(parsePort(" 1993 ")).toBe(1993);
  });
});

describe("adding a mailbox tries it for real before keeping it", () => {
  test("Add sends the mailbox to the command that logs in, then stores", async () => {
    // GIVEN a runtime whose login succeeds
    const calls: Call[] = [];
    const account = {
      connectionId: "imap:ana@gmail.com",
      email: "ana@gmail.com",
      host: "imap.gmail.com",
      port: 993,
      username: "ana@gmail.com",
    };
    fakeInvoke(calls, { add_imap_mailbox: account });

    // WHEN the form is added
    const outcome = await addMailbox(form({ email: " ana@gmail.com " }));

    // THEN one call carries the address trimmed, the server, the port and the
    // password, and the connection to show in the list comes back
    expect(outcome).toEqual({ kind: "added", account });
    expect(calls).toEqual([
      {
        cmd: "add_imap_mailbox",
        args: {
          mailbox: {
            email: "ana@gmail.com",
            host: "imap.gmail.com",
            port: 993,
            password: "abcd efgh ijkl mnop",
          },
        },
      },
    ]);
  });

  test("each failure of the trial is told in plain words", async () => {
    // GIVEN the runtime's own messages for each failure of a mailbox
    const cases: [string, string][] = [
      ["the server refused the identifier or the password", "credentials_rejected"],
      ["imap.gmail.com:993 cannot be reached: connection refused", "unreachable"],
      ["the secure connection to imap.gmail.com failed: invalid certificate", "tls"],
      ["`ana` is not an email address", "invalid_email"],
      ["no IMAP host is known for `exemple.fr`; give the server address", "invalid_host"],
      ["port 0 is not a server port", "invalid_port"],
      ["the password is empty or too long", "invalid_password"],
      ["the server answered outside the protocol: BYE", "protocol"],
      ["the password could not be kept in the secret store", "secret_store"],
      ["failed to connect to runtime API: refused", "runtime"],
      ["something nobody expected", "generic"],
    ];

    for (const [message, code] of cases) {
      // WHEN the addition is refused with that message
      fakeInvoke([], {
        add_imap_mailbox: () => {
          throw message;
        },
      });
      const outcome = await addMailbox(form());

      // THEN the outcome names a text present in both languages, never the raw message
      const key = `connections.mailbox.error.${code}`;
      expect(outcome).toEqual({ kind: "failed", messageKey: key, detail: message });
      expect(mailboxErrorKey(message)).toBe(key);
      expect(typeof lookup(fr, key), key).toBe("string");
      expect(typeof lookup(en, key), key).toBe("string");
      expect(lookup(fr, key)).not.toBe(message);
    }
  });

  test("a refusal of the organisation policy says who refused", async () => {
    // GIVEN an admission that refuses online connections
    fakeInvoke([], {
      add_imap_mailbox: () => {
        throw REMOTE_DENIED_REASON;
      },
    });

    // WHEN the mailbox is added
    const outcome = await addMailbox(form());

    // THEN it is a refusal by the organisation, not a failed trial
    expect(outcome).toEqual({
      kind: "refused",
      refusal: { code: "remote_denied", by: "organisation", reason: REMOTE_DENIED_REASON },
    });
  });
});

describe("the plain words follow the runtime's own messages", () => {
  test("each failure an addition can meet is recognised from the text the runtime writes", () => {
    // GIVEN the messages the mailbox errors are declared with, placeholders filled
    const source = readFileSync(
      new URL("../../../../../apollia-connectors/src/imap/error.rs", import.meta.url),
      "utf8",
    );
    const declared = new Map<string, string>();
    for (const match of source.matchAll(/#\[error\("((?:[^"\\]|\\.)*)"\)\]\s*(\w+)/g)) {
      declared.set(match[2], match[1].replace(/\{[^}]*\}/g, "x"));
    }
    const expected: Record<string, string> = {
      InvalidEmail: "invalid_email",
      UnknownProvider: "invalid_host",
      InvalidHost: "invalid_host",
      InvalidPort: "invalid_port",
      Unreachable: "unreachable",
      Tls: "tls",
      CredentialsRejected: "credentials_rejected",
      Protocol: "protocol",
      Secret: "secret_store",
    };

    // WHEN each declared message is mapped
    // THEN it reaches its own plain-language text, never the generic one
    for (const [variant, code] of Object.entries(expected)) {
      const message = declared.get(variant);
      expect(message, variant).toBeDefined();
      expect(mailboxErrorKey(message ?? ""), variant).toBe(`connections.mailbox.error.${code}`);
    }
  });
});
