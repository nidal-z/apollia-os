import { describe, test, expect, vi, beforeEach } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import * as decisions from "./serverTokens";
import {
  CLOSED_WINDOW,
  LONGEST_TOKEN_NAME,
  SERVER_TOKENS_TEXTS,
  activeTokenCount,
  closeTokens,
  copyShownToken,
  createToken,
  openTokens,
  parseTokenName,
  revokeToken,
  tokensInUse,
  withActiveTokens,
  type ServerTokensIpc,
  type TokensWindow,
} from "./serverTokens";
import { SERVER_SETTINGS_TEXTS } from "./serverSettings";
import type { ServerBarState } from "./localServer";
import {
  createServerToken,
  listServerTokens,
  revokeServerToken,
  type CreatedServerToken,
  type ServerToken,
  type ServerTokenRefusal,
} from "$lib/ipc/serverTokens";
import type { LocalServerView } from "$lib/ipc/localServer";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";

const HERE = resolve(__dirname);
const SRC = resolve(__dirname, "../..");

const VALUE = `apo_${"0".repeat(60)}1e8c`;

function text(catalogue: unknown, key: string): string {
  const value = key
    .split(".")
    .reduce<unknown>(
      (node, part) =>
        node && typeof node === "object" ? (node as Record<string, unknown>)[part] : undefined,
      catalogue,
    );
  if (typeof value !== "string") throw new Error(`missing ${key}`);
  return value;
}

function source(name: string): string {
  return readFileSync(join(HERE, name), "utf8");
}

/** Every source file of the interface that is not a test. */
function sources(dir: string = SRC): string[] {
  return readdirSync(dir).flatMap((entry) => {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) return sources(path);
    if (!/\.(ts|svelte)$/.test(entry) || /\.(test|d)\.ts$/.test(entry)) return [];
    return [path];
  });
}

function token(id: number, name: string, over: Partial<ServerToken> = {}): ServerToken {
  return {
    id,
    name,
    lastFour: "a3f9",
    createdAt: "2026-09-28T10:00:00Z",
    lastUsedAt: null,
    revoked: false,
    ...over,
  };
}

/** A runtime holding its tokens in memory, as the three routes answer. */
function memoryRuntime(initial: ServerToken[] = []) {
  const rows = initial.map((row) => ({ ...row }));
  const ipc: ServerTokensIpc = {
    list: vi.fn(async () => rows.map((row) => ({ ...row }))),
    create: vi.fn(async (name: string): Promise<CreatedServerToken> => {
      if (rows.some((row) => row.name === name && !row.revoked)) {
        const refusal: ServerTokenRefusal = { kind: "name_taken", message: "name taken" };
        throw refusal;
      }
      const row = token(rows.length + 1, name, { lastFour: VALUE.slice(-4) });
      rows.push(row);
      return { ...row, value: VALUE };
    }),
    revoke: vi.fn(async (id: number) => {
      const row = rows.find((candidate) => candidate.id === id);
      if (!row) throw new Error("no token matches");
      row.revoked = true;
      return { ...row };
    }),
  };
  return { ipc, rows };
}

function view(activeTokens: number): LocalServerView {
  return {
    running: true,
    port: 7772,
    serveOnLan: false,
    address: "http://127.0.0.1:7772",
    lastError: null,
    activeTokens,
    imposedByPolicy: { serveOnLan: false, requireAuth: false, allowRequestMcp: false },
  };
}

beforeEach(() => {
  invoke.mockReset();
});

describe("Manage tokens opens the window that lists the tokens", () => {
  test("the button of the settings closes the panel and opens the window", () => {
    // GIVEN the panel of the settings and the bar that holds it
    const panel = source("ServerSettingsPanel.svelte");
    const bar = source("ServerBar.svelte");

    // WHEN the button "Manage tokens" is read
    const button = panel.slice(panel.indexOf('data-testid="developer-server-manage-tokens"') - 200);

    // THEN it is never disabled and asks the bar for the window
    expect(button.slice(0, 200)).toContain("onclick={onmanagetokens}");
    expect(button.slice(0, 200)).not.toContain("disabled");
    // AND the bar closes the settings and opens the window of the tokens
    expect(bar).toContain("onmanagetokens={manageTokens}");
    expect(bar).toMatch(
      /function manageTokens\(\): void \{\s*settingsOpen = false;\s*tokensOpen = true;/,
    );
    expect(bar).toContain("bind:open={settingsOpen}");
    expect(bar).toMatch(/<ServerTokensDialog\s+open=\{tokensOpen\}/);
  });

  test("the window is read from the route of the tokens, through the shell", async () => {
    // GIVEN a shell that answers the list
    invoke.mockResolvedValueOnce([token(1, "crm")]);

    // WHEN the window opens on the real commands
    const opened = await openTokens({
      list: listServerTokens,
      create: createServerToken,
      revoke: revokeServerToken,
    });

    // THEN the list command was called, and its rows are the window's
    expect(invoke).toHaveBeenCalledWith("list_server_tokens");
    expect(opened.read).toBe(true);
    expect(opened.tokens.map((row) => row.name)).toEqual(["crm"]);
    expect(opened.shown).toBeNull();
  });

  test("the window lists the tokens in use, a revoked one left out", async () => {
    // GIVEN a runtime with two tokens in use and one revoked
    const { ipc } = memoryRuntime([
      token(1, "crm", { lastUsedAt: "2026-10-01T08:00:00Z" }),
      token(2, "old", { revoked: true }),
      token(3, "dashboard", { lastFour: "71c2" }),
    ]);

    // WHEN the window opens
    const opened = await openTokens(ipc);

    // THEN it lists the two in use, oldest first, and counts them
    expect(tokensInUse(opened).map((row) => row.name)).toEqual(["crm", "dashboard"]);
    expect(activeTokenCount(opened)).toBe(2);
  });

  test("a row shows the name, the last four characters, the creation and the last use", () => {
    // GIVEN the component of the window
    const dialog = source("ServerTokensDialog.svelte");
    const row = dialog.slice(dialog.indexOf("{#each listed as token"), dialog.indexOf("{/each}"));

    // WHEN the row of a token is read
    // THEN it renders the four facts and its own test id
    expect(row).toContain("{token.name}");
    expect(row).toContain("{token.lastFour}");
    expect(row).toContain("SERVER_TOKENS_TEXTS.createdOn");
    expect(row).toContain("createdOn(token)");
    expect(row).toContain("formatRelativeTime(token.lastUsedAt, language)");
    expect(row).toContain("SERVER_TOKENS_TEXTS.neverUsed");
    expect(row).toContain("data-testid={`developer-server-token-row-${token.id}`}");
    expect(row).toContain("data-testid={`developer-server-token-row-${token.id}-revoke`}");
  });

  test("a list that cannot be read says so and tells no count", async () => {
    // GIVEN a runtime whose list fails
    const ipc: ServerTokensIpc = {
      list: vi.fn(async () => {
        throw new Error("API error (503): no compatible server");
      }),
      create: vi.fn(),
      revoke: vi.fn(),
    };

    // WHEN the window opens
    const opened = await openTokens(ipc);

    // THEN the failure carries the runtime's words and no count is told
    expect(opened.failure).toEqual({
      kind: "read",
      message: "API error (503): no compatible server",
    });
    expect(activeTokenCount(opened)).toBeNull();
  });
});

describe("a token is created with a name and its value is shown once", () => {
  test("creating shows the value, with the name, and lists the token", async () => {
    // GIVEN an open window on a runtime with one token
    const { ipc } = memoryRuntime([token(1, "crm")]);
    const opened = await openTokens(ipc);

    // WHEN a token is created under a name typed with spaces around
    const created = await createToken(ipc, opened, "  test script ");

    // THEN the runtime was given the trimmed name
    expect(ipc.create).toHaveBeenCalledWith("test script");
    // AND the window shows the value once, and lists the token without it
    expect(created.shown).toEqual({ id: 2, name: "test script", value: VALUE });
    expect(tokensInUse(created).map((row) => row.name)).toEqual(["crm", "test script"]);
    expect(JSON.stringify(created.tokens)).not.toContain(VALUE);
    expect(activeTokenCount(created)).toBe(2);
    expect(created.failure).toBeNull();
  });

  test("the creation goes through the shell with the name", async () => {
    // GIVEN a shell that answers a created token
    invoke.mockResolvedValueOnce({ ...token(4, "script"), value: VALUE });

    // WHEN the wrapper is called
    const created = await createServerToken("script");

    // THEN the command took the name and answered the value
    expect(invoke).toHaveBeenCalledWith("create_server_token", { name: "script" });
    expect(created.value).toBe(VALUE);
  });

  test("the value comes with a copy button and the warning that it is not shown again", () => {
    // GIVEN the component of the window
    const dialog = source("ServerTokensDialog.svelte");
    const shown = dialog.slice(dialog.indexOf("{#if current.shown}"), dialog.indexOf("{/if}"));

    // WHEN the block of a created token is read
    // THEN it holds the value, the copy button and the warning
    expect(shown).toContain('data-testid="developer-server-token-value">{current.shown.value}');
    expect(shown).toContain('data-testid="developer-server-token-copy"');
    expect(dialog).toContain("SERVER_TOKENS_TEXTS.shownOnce");
    // AND the warning says it, in a French of its own
    expect(text(en, SERVER_TOKENS_TEXTS.shownOnce)).toMatch(/never be shown again/);
    expect(text(fr, SERVER_TOKENS_TEXTS.shownOnce)).not.toBe(
      text(en, SERVER_TOKENS_TEXTS.shownOnce),
    );
  });

  test("the copy button copies the value shown, and nothing when none is", async () => {
    // GIVEN a window that shows a token and one that shows none
    const { ipc } = memoryRuntime();
    const created = await createToken(ipc, await openTokens(ipc), "script");
    const write = vi.fn(async () => {});

    // WHEN each is copied
    const done = await copyShownToken(created, write);
    const nothing = await copyShownToken(closeTokens(), write);

    // THEN the value went to the clipboard once
    expect(done).toBe(true);
    expect(nothing).toBe(false);
    expect(write).toHaveBeenCalledTimes(1);
    expect(write).toHaveBeenCalledWith(VALUE);
    // AND a clipboard that refuses is reported
    const refused = await copyShownToken(created, async () => {
      throw new Error("denied");
    });
    expect(refused).toBe(false);
  });

  test("closing the window clears the value from the interface", async () => {
    // GIVEN a window that shows the value of a token just created
    const { ipc } = memoryRuntime();
    const created = await createToken(ipc, await openTokens(ipc), "script");
    expect(created.shown?.value).toBe(VALUE);

    // WHEN the window closes
    const closed = closeTokens();

    // THEN what is left holds no value, and the closed state was never written to
    expect(closed.shown).toBeNull();
    expect(JSON.stringify(closed)).not.toContain(VALUE);
    expect(CLOSED_WINDOW).toEqual({ tokens: [], read: false, shown: null, failure: null });
    // AND the component drops its state, the typed name included, when it closes
    const dialog = source("ServerTokensDialog.svelte");
    expect(dialog).toMatch(/if \(!open\) \{[^}]*current = closeTokens\(\);\s*name = "";/);
  });

  test("reopening the window lists the token and never its value", async () => {
    // GIVEN a token created in a window since closed
    const { ipc } = memoryRuntime();
    await createToken(ipc, await openTokens(ipc), "script");

    // WHEN the window opens again
    const reopened = await openTokens(ipc);

    // THEN the token is listed by its name and its end, the value is not there
    expect(tokensInUse(reopened).map((row) => row.name)).toEqual(["script"]);
    expect(reopened.shown).toBeNull();
    expect(JSON.stringify(reopened)).not.toContain(VALUE);
  });

  test("no store keeps the value", async () => {
    // GIVEN the value of a token created through the decisions of the window
    const { ipc } = memoryRuntime();
    await createToken(ipc, await openTokens(ipc), "script");

    // WHEN what the module of decisions holds after the fact is read
    const held = JSON.stringify(Object.values(decisions).filter((item) => typeof item !== "function"));

    // THEN nothing of it carries the value
    expect(held).not.toContain(VALUE);

    // AND the two files that see the value keep it out of every store: no
    // store is written, no browser storage is touched, and the only store
    // read is the switch of the developer mode
    for (const name of ["serverTokens.ts", "ServerTokensDialog.svelte"]) {
      const file = source(name);
      expect(file, name).not.toMatch(/from "svelte\/store"/);
      expect(file, name).not.toMatch(/localStorage|sessionStorage|indexedDB/);
      expect(file, name).not.toMatch(/\bwritable\(|\.set\(|\.update\(/);
      const stores = [...file.matchAll(/from "\$lib\/stores\/([^"]+)"/g)].map((match) => match[1]);
      expect(stores, name).toEqual(name.endsWith(".svelte") ? ["developerMode"] : []);
    }

    // AND no other source file of the interface asks for a creation, so no
    // other file ever holds a value
    const callers = sources()
      .filter((path) => /createServerToken|create_server_token/.test(readFileSync(path, "utf8")))
      .map((path) => relative(SRC, path))
      .sort();
    expect(callers).toEqual([
      "components/developer/ServerTokensDialog.svelte",
      "lib/ipc/serverTokens.ts",
    ]);
  });

  test("a name the runtime would refuse is said and nothing is sent", async () => {
    // GIVEN an open window
    const { ipc } = memoryRuntime();
    const opened = await openTokens(ipc);

    // WHEN an empty name, a name of digits and a name too long are tried
    const empty = await createToken(ipc, opened, "   ");
    const digits = await createToken(ipc, opened, "12345");
    const long = await createToken(ipc, opened, "n".repeat(LONGEST_TOKEN_NAME + 1));

    // THEN each is refused with its reason, before any call
    expect(empty.failure).toEqual({ kind: "name_empty" });
    expect(digits.failure).toEqual({ kind: "name_invalid" });
    expect(long.failure).toEqual({ kind: "name_invalid" });
    expect(ipc.create).not.toHaveBeenCalled();
    expect(parseTokenName("n".repeat(LONGEST_TOKEN_NAME))).toBe("n".repeat(LONGEST_TOKEN_NAME));
  });

  test("a name a token in use carries is refused, and the window keeps what it showed", async () => {
    // GIVEN a window that shows a token just created
    const { ipc } = memoryRuntime();
    const created = await createToken(ipc, await openTokens(ipc), "script");

    // WHEN the same name is asked again
    const again = await createToken(ipc, created, "script");

    // THEN the refusal names it, and the list and the value shown stay
    expect(again.failure).toEqual({ kind: "name_taken", name: "script" });
    expect(again.tokens).toEqual(created.tokens);
    expect(again.shown).toEqual(created.shown);
  });

  test("another failure of the creation carries the runtime's words", async () => {
    // GIVEN a runtime that refuses for a reason of its own
    const ipc: ServerTokensIpc = {
      list: vi.fn(async () => []),
      create: vi.fn(async () => {
        const refusal: ServerTokenRefusal = { kind: "other", message: "the token store failed" };
        throw refusal;
      }),
      revoke: vi.fn(),
    };

    // WHEN a token is asked
    const failed = await createToken(ipc, await openTokens(ipc), "script");

    // THEN the failure says it
    expect(failed.failure).toEqual({ kind: "create", message: "the token store failed" });
    expect(failed.shown).toBeNull();
  });
});

describe("a token is revoked after a confirmation, and the count follows", () => {
  test("the button of a row asks, and only the confirmation revokes", () => {
    // GIVEN the component of the window
    const dialog = source("ServerTokensDialog.svelte");

    // WHEN the ways to the revocation are read
    const row = dialog.slice(dialog.indexOf("{#each listed as token"), dialog.indexOf("{/each}"));
    const calls = [...dialog.matchAll(/void revoke\(\)/g)];

    // THEN the row only names the token asked about
    expect(row).toContain("onclick={() => (asked = token)}");
    expect(row).not.toContain("revoke()");
    // AND the one call to the revocation is the confirm of the dialog
    expect(calls).toHaveLength(1);
    const confirm = dialog.slice(dialog.indexOf("<ConfirmDialog"));
    expect(confirm).toContain("onconfirm={() => void revoke()}");
    expect(confirm).toContain('data-testid="developer-server-token-revoke"');
  });

  test("revoking takes the token out of the list and lowers the count", async () => {
    // GIVEN an open window on two tokens
    const { ipc, rows } = memoryRuntime([token(1, "crm"), token(2, "dashboard")]);
    const opened = await openTokens(ipc);

    // WHEN the first is revoked
    const after = await revokeToken(ipc, opened, 1);

    // THEN the runtime revoked it, the list lost it and the count is one
    expect(ipc.revoke).toHaveBeenCalledWith(1);
    expect(rows[0].revoked).toBe(true);
    expect(tokensInUse(after).map((row) => row.name)).toEqual(["dashboard"]);
    expect(activeTokenCount(after)).toBe(1);
    expect(after.failure).toBeNull();
  });

  test("the revocation goes through the shell with the identifier", async () => {
    // GIVEN a shell that answers the token, revoked
    invoke.mockResolvedValueOnce(token(2, "dashboard", { revoked: true }));

    // WHEN the wrapper is called
    const revoked = await revokeServerToken(2);

    // THEN the command took the identifier
    expect(invoke).toHaveBeenCalledWith("revoke_server_token", { id: 2 });
    expect(revoked.revoked).toBe(true);
  });

  test("Active tokens: N follows a creation and a revocation", async () => {
    // GIVEN a bar that read one token in use, and the window open on it
    const { ipc } = memoryRuntime([token(1, "crm")]);
    let bar: ServerBarState = { view: view(1), failure: null };
    let current: TokensWindow = await openTokens(ipc);
    const follow = () => {
      const count = activeTokenCount(current);
      if (count !== null) bar = withActiveTokens(bar, count);
    };

    // WHEN a token is created, then both are revoked
    current = await createToken(ipc, current, "script");
    follow();
    const afterCreation = bar.view?.activeTokens;
    current = await revokeToken(ipc, current, 1);
    follow();
    const afterFirst = bar.view?.activeTokens;
    current = await revokeToken(ipc, current, 2);
    follow();

    // THEN the count the settings show went two, one, none
    expect(afterCreation).toBe(2);
    expect(afterFirst).toBe(1);
    expect(bar.view?.activeTokens).toBe(0);
    // AND the settings render that count, which the bar takes from the window
    const panel = source("ServerSettingsPanel.svelte");
    expect(panel).toContain(
      "$t(SERVER_SETTINGS_TEXTS.activeTokens, { values: { count: view?.activeTokens ?? 0 } })",
    );
    expect(text(fr, SERVER_SETTINGS_TEXTS.activeTokens)).toContain("{count}");
    const serverBar = source("ServerBar.svelte");
    expect(serverBar).toContain("bar = withActiveTokens(bar, count);");
    expect(serverBar).toContain("oncount={takeTokenCount}");
    expect(source("ServerTokensDialog.svelte")).toMatch(
      /const count = activeTokenCount\(next\);\s*if \(count !== null\) oncount\(count\);/,
    );
  });

  test("the count leaves the rest of the bar alone, and a bar with no state stays so", () => {
    // GIVEN a bar with a failure, and a bar that read nothing
    const failed: ServerBarState = { view: view(3), failure: { kind: "other", message: "x" } };
    const empty: ServerBarState = { view: null, failure: null };

    // WHEN a count is told to each
    const told = withActiveTokens(failed, 4);

    // THEN only the count changed
    expect(told).toEqual({ view: { ...view(3), activeTokens: 4 }, failure: failed.failure });
    expect(withActiveTokens(empty, 4)).toBe(empty);
    expect(withActiveTokens(failed, 3)).toBe(failed);
  });

  test("revoking the token whose value is shown takes the value away with it", async () => {
    // GIVEN a window that shows a token just created
    const { ipc } = memoryRuntime([token(1, "crm")]);
    const created = await createToken(ipc, await openTokens(ipc), "script");

    // WHEN another token is revoked, then the one shown
    const other = await revokeToken(ipc, created, 1);
    const shown = await revokeToken(ipc, other, 2);

    // THEN the value stayed for the first and left with the second
    expect(other.shown?.value).toBe(VALUE);
    expect(shown.shown).toBeNull();
    expect(JSON.stringify(shown)).not.toContain(VALUE);
  });

  test("a revocation that fails says why and changes nothing", async () => {
    // GIVEN a window listing a token the runtime no longer knows
    const { ipc } = memoryRuntime([token(1, "crm")]);
    const opened = await openTokens(ipc);

    // WHEN an unknown token is revoked
    const failed = await revokeToken(ipc, opened, 9);

    // THEN the list is the same and the failure carries the reason
    expect(failed.tokens).toEqual(opened.tokens);
    expect(failed.failure).toEqual({ kind: "revoke", message: "no token matches" });
  });
});

describe("the window speaks both languages and is driven by test ids", () => {
  test("every text of the window exists in French and in English", () => {
    // GIVEN the keys the window renders
    const keys = Object.values(SERVER_TOKENS_TEXTS);

    // WHEN each is read in both catalogues
    const same = keys.filter((key) => text(fr, key) === text(en, key));
    const empty = keys.filter((key) => text(fr, key).trim() === "" || text(en, key).trim() === "");

    // THEN none is missing, empty, or left untranslated
    expect(empty).toEqual([]);
    expect(same).toEqual([]);
    // AND the catalogue holds no text of the window that it does not render
    const catalogued = Object.keys(
      (fr as unknown as { dev: { developer: { tokens: Record<string, string> } } }).dev.developer
        .tokens,
    ).map((key) => `dev.developer.tokens.${key}`);
    expect(catalogued.sort()).toEqual([...keys].sort());
  });

  test("every key of the window is rendered by the component", () => {
    // GIVEN the component and the names of its texts
    const dialog = source("ServerTokensDialog.svelte");

    // WHEN each name is looked for in it
    const unused = Object.keys(SERVER_TOKENS_TEXTS).filter(
      (name) => !dialog.includes(`SERVER_TOKENS_TEXTS.${name}`),
    );

    // THEN none is left out, and no text is written in the markup
    expect(unused).toEqual([]);
  });

  test("every control of the window carries a test id", () => {
    // GIVEN the component
    const dialog = source("ServerTokensDialog.svelte");

    // WHEN its controls are counted against their test ids
    const controls = [...dialog.matchAll(/<(Button|Input)\b[\s\S]*?>\s*\n/g)].map((match) => match[0]);
    const bare = controls.filter((control) => !control.includes("data-testid="));

    // THEN each has one, and the window is only shown in developer mode
    expect(controls.length).toBeGreaterThanOrEqual(5);
    expect(bare).toEqual([]);
    expect(dialog).toContain("{#if $developerMode}");
  });
});
