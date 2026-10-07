import { describe, test, expect, vi, beforeEach } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { get } from "svelte/store";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

import {
  HIGHEST_PORT,
  LOWEST_PORT,
  SERVER_SETTINGS_TEXTS,
  SERVER_SETTING_ROWS,
  appliesAtReopening,
  barFailure,
  changeMinutes,
  changePort,
  changeServerSetting,
  imposedByPolicy,
  parsePort,
  rowDisabled,
  servedOnLanWithoutAuth,
  settingTestId,
  type SettingChangeDeps,
} from "./serverSettings";
import { ON_DEMAND_ROWS, SEPARATE_REASONING_ROW } from "../settings/developer/onDemandLoading";
import {
  SERVER_SETTING_DEFAULTS,
  SERVER_SETTING_KEYS,
  hydrateServerSettings,
  serverSettings,
  setServerSetting,
  type ServerSettingKey,
  type ServerSettingsIpc,
} from "$lib/stores/serverSettings";
import type { ResolvedSetting } from "$lib/ipc/settings";
import type { LocalServerRefusal, LocalServerView } from "$lib/ipc/localServer";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";

const HERE = resolve(__dirname);
const KEY_REGISTRY = resolve(__dirname, "../../../../../apollia-core/src/config/keys.rs");

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

/** A shell holding one `apollia.toml` in memory, policy keys refused. */
function memoryShell(policy: Record<string, unknown> = {}) {
  const file = new Map<string, unknown>();
  const ipc: ServerSettingsIpc = {
    read: vi.fn(async (key: string): Promise<ResolvedSetting> => {
      if (key in policy) return { key, value: policy[key], source: "policy" };
      if (file.has(key)) return { key, value: file.get(key), source: "user" };
      return { key, value: SERVER_SETTING_DEFAULTS[key as ServerSettingKey], source: "default" };
    }),
    write: vi.fn(async (key: string, value: unknown): Promise<ResolvedSetting> => {
      if (key in policy) throw new Error("locked by policy");
      file.set(key, value);
      return { key, value, source: "user" };
    }),
  };
  return { file, ipc };
}

function view(over: Partial<LocalServerView> = {}): LocalServerView {
  return {
    running: false,
    port: 7772,
    serveOnLan: false,
    address: "http://127.0.0.1:7772",
    lastError: null,
    activeTokens: 0,
    imposedByPolicy: { serveOnLan: false, requireAuth: false, allowRequestMcp: false },
    ...over,
  };
}

/**
 * What a change reaches: the memory shell for the write, and a runtime whose
 * reopening succeeds, or is refused for a taken port, or fails otherwise.
 */
function deps(
  ipc: ServerSettingsIpc,
  opts: { running?: boolean; taken?: boolean; broken?: boolean } = {},
) {
  const before = view({ running: opts.running ?? false });
  const restart = vi.fn(async (): Promise<LocalServerView> => {
    if (opts.taken) {
      const refusal: LocalServerRefusal = {
        kind: "port_in_use",
        message: "port 8080 is already in use by another program",
      };
      throw refusal;
    }
    if (opts.broken) throw new Error("failed to listen");
    return view({ running: true, port: Number(get(serverSettings)["server.port"].value) });
  });
  const read = vi.fn(async () => view({ running: false, lastError: "refused" }));
  const all: SettingChangeDeps = {
    write: (key, value) => setServerSetting(key, value, ipc),
    restart,
    read,
    view: before,
  };
  return { all, restart, read };
}

/** A value of the type of `key` that differs from its default. */
function otherValue(key: ServerSettingKey): boolean | number {
  const standard = SERVER_SETTING_DEFAULTS[key];
  if (typeof standard === "boolean") return !standard;
  return key === SERVER_SETTING_KEYS.port ? 8080 : 15;
}

beforeEach(async () => {
  // Every test starts from the defaults, as a fresh window would.
  await hydrateServerSettings(memoryShell().ipc);
});

describe("the server settings: the rows of the panel", () => {
  test("the rows come in the order of the reference, the delay under auto-unload", () => {
    // GIVEN the rows of the panel
    // WHEN their keys are read in order
    const keys = SERVER_SETTING_ROWS.map((row) => row.key);

    // THEN port, token check, local network, request MCP, CORS, on-demand
    // loading, auto-unload and its delay, keep the last only, reasoning
    expect(keys).toEqual([
      "server.port",
      "server.require_auth",
      "server.serve_on_lan",
      "server.allow_request_mcp",
      "server.cors",
      "server.jit_loading",
      "server.jit_auto_unload",
      "server.jit_idle_ttl_minutes",
      "server.jit_keep_last_only",
      "server.separate_reasoning_content",
    ]);
    const delay = SERVER_SETTING_ROWS[7]!;
    expect(delay.under).toBe("server.jit_auto_unload");
    expect(SERVER_SETTING_ROWS.filter((row) => row.under).length).toBe(1);
  });

  test("every setting of the store has its row, and each row its own test id", () => {
    // GIVEN the keys of the shared store
    const stored = Object.values(SERVER_SETTING_KEYS).sort();

    // WHEN the rows are compared with them
    const shown = SERVER_SETTING_ROWS.map((row) => row.key).sort();
    const ids = SERVER_SETTING_ROWS.map(settingTestId);

    // THEN no setting is left out, and no test id is shared
    expect(shown).toEqual(stored);
    expect(new Set(ids).size).toBe(ids.length);
    for (const id of ids) expect(id).toMatch(/^developer-server-setting-[a-z-]+$/);
  });

  test.each(SERVER_SETTING_ROWS.map((row) => [row.key, row] as const))(
    "the control of %s reads and writes its key through the shell",
    async (key, row) => {
      // GIVEN one apollia.toml behind the shell, holding a value for the key
      const { file, ipc } = memoryShell();
      const held = otherValue(key);
      file.set(key, held);

      // WHEN the panel opens, THEN the control reads that value
      await hydrateServerSettings(ipc);
      expect(get(serverSettings)[key]).toEqual({ value: held, source: "user" });

      // WHEN the control is set back to the default
      const { all } = deps(ipc);
      const done = await changeServerSetting(key, SERVER_SETTING_DEFAULTS[key], all);

      // THEN the shell wrote that one key, and the store carries it
      expect(done.failure).toBeNull();
      expect(ipc.write).toHaveBeenCalledTimes(1);
      expect(ipc.write).toHaveBeenCalledWith(key, SERVER_SETTING_DEFAULTS[key]);
      expect(file.get(key)).toBe(SERVER_SETTING_DEFAULTS[key]);
      expect(get(serverSettings)[key].value).toBe(SERVER_SETTING_DEFAULTS[key]);
      // AND the registry of the engine declares the key the row writes
      expect(readFileSync(KEY_REGISTRY, "utf8")).toContain(`key("${row.key}"`);
    },
  );

  test("the panel draws every row from the list, with the count of tokens and their button", () => {
    // GIVEN the source of the panel
    const panel = source("ServerSettingsPanel.svelte");

    // WHEN what it draws is read
    // THEN the rows come from the list, in its order, each through the row
    // component; the token check carries the count and "Manage tokens"
    expect(panel).toContain("{#each SERVER_SETTING_ROWS as item (item.key)}");
    expect(panel.match(/<ServerSettingRow/g)).toHaveLength(1);
    expect(panel).toContain('data-testid="developer-server-active-tokens"');
    expect(panel).toContain('data-testid="developer-server-manage-tokens"');
    expect(text(fr, SERVER_SETTINGS_TEXTS.activeTokens)).toContain("{count}");
    expect(text(en, SERVER_SETTINGS_TEXTS.activeTokens)).toBe("Active tokens: {count}");
    expect(text(en, SERVER_SETTINGS_TEXTS.manageTokens)).toBe("Manage tokens");
    expect(text(fr, SERVER_SETTINGS_TEXTS.manageTokens)).not.toBe("Manage tokens");
    // AND the bar opens it, next to the switch
    expect(source("ServerBar.svelte")).toContain("<ServerSettingsPanel");
    expect(text(fr, SERVER_SETTINGS_TEXTS.open)).not.toBe("Server settings");
    expect(text(en, SERVER_SETTINGS_TEXTS.open)).toBe("Server settings");
  });
});

describe("the server settings: a running server applies what it reads at its opening", () => {
  test("the port, the token check, the local network and CORS reopen a running server", async () => {
    // GIVEN a running server
    for (const key of Object.values(SERVER_SETTING_KEYS)) {
      const { ipc } = memoryShell();
      const { all, restart } = deps(ipc, { running: true });

      // WHEN the setting changes
      const done = await changeServerSetting(key, otherValue(key), all);

      // THEN it is reopened for the four the listener reads at its opening,
      // and left alone for those read at every request
      expect(done.failure).toBeNull();
      expect(restart).toHaveBeenCalledTimes(appliesAtReopening(key) ? 1 : 0);
      expect(done.view?.running).toBe(appliesAtReopening(key) ? true : undefined);
    }
    expect(Object.values(SERVER_SETTING_KEYS).filter(appliesAtReopening)).toEqual([
      "server.port",
      "server.require_auth",
      "server.serve_on_lan",
      "server.cors",
    ]);
  });

  test("a stopped server is not started by a change", async () => {
    // GIVEN a stopped server
    const { ipc } = memoryShell();
    const { all, restart } = deps(ipc, { running: false });

    // WHEN the local network is turned on
    const done = await changeServerSetting(SERVER_SETTING_KEYS.serveOnLan, true, all);

    // THEN the key is written and nothing is opened
    expect(done).toEqual({ failure: null });
    expect(restart).not.toHaveBeenCalled();
  });

  test("a write the shell refuses reopens nothing and is said", async () => {
    // GIVEN a running server and a key the policy imposes
    const { ipc } = memoryShell({ "server.cors": false });
    await hydrateServerSettings(ipc);
    const { all, restart } = deps(ipc, { running: true });

    // WHEN CORS is turned on
    const done = await changeServerSetting(SERVER_SETTING_KEYS.cors, true, all);

    // THEN the change fails as a refused write, the server untouched
    expect(done).toEqual({ failure: { kind: "write" } });
    expect(restart).not.toHaveBeenCalled();
    expect(ipc.write).not.toHaveBeenCalled();
  });
});

describe("the server settings: the warning of the bar", () => {
  test("the bar warns while the server answers the local network without a token", () => {
    // GIVEN a server on the local network
    const onLan = view({ running: true, serveOnLan: true });

    // WHEN the token check is off, then on; the server stopped; on the machine alone
    // THEN only the first warns
    expect(servedOnLanWithoutAuth(onLan, false)).toBe(true);
    expect(servedOnLanWithoutAuth(onLan, true)).toBe(false);
    expect(servedOnLanWithoutAuth(view({ running: false, serveOnLan: true }), false)).toBe(false);
    expect(servedOnLanWithoutAuth(view({ running: true, serveOnLan: false }), false)).toBe(false);
    expect(servedOnLanWithoutAuth(null, false)).toBe(false);
  });

  test("the warning says agents require a token from the network, and cannot be dismissed", () => {
    // GIVEN the texts of the warning and the source of the bar
    const bar = source("ServerBar.svelte");

    // WHEN they are read
    // THEN both languages name the token agents and workflows require
    expect(text(fr, SERVER_SETTINGS_TEXTS.lanWarning)).toMatch(/agents.*workflows.*jeton/);
    expect(text(en, SERVER_SETTINGS_TEXTS.lanWarning)).toMatch(/agents and workflows always require a token/);
    // AND the bar shows it for as long as the condition holds: nothing closes it
    expect(bar).toContain("{#if lanWithoutAuth}");
    expect(bar).toContain('data-testid="developer-server-lan-warning"');
    const warning = bar.slice(bar.indexOf("{#if lanWithoutAuth}"), bar.indexOf("{#if failure}"));
    expect(warning).not.toMatch(/dismiss|onclose|close\(/i);
    expect(warning).toContain('data-testid="developer-server-lan-require-auth"');
  });

  test("the way out of the warning requires a token and reopens the server", async () => {
    // GIVEN a running server on the local network, without a token check
    const { file, ipc } = memoryShell();
    const { all, restart } = deps(ipc, { running: true });

    // WHEN "Require authentication" is pressed
    const done = await changeServerSetting(SERVER_SETTING_KEYS.requireAuth, true, all);

    // THEN the key is on and the listener reopened to apply it
    expect(file.get("server.require_auth")).toBe(true);
    expect(restart).toHaveBeenCalledTimes(1);
    expect(done.failure).toBeNull();
  });
});

describe("the server settings: a setting the policy imposes", () => {
  test("a key the shell resolves from the policy is locked", async () => {
    // GIVEN a policy that imposes on-demand loading off
    const { ipc } = memoryShell({ "server.jit_loading": false });
    await hydrateServerSettings(ipc);
    const settings = get(serverSettings);
    const row = SERVER_SETTING_ROWS.find((item) => item.key === "server.jit_loading")!;

    // WHEN the row is asked whether it can be set
    // THEN it is imposed, its control disabled, and a write is refused
    expect(imposedByPolicy(row.key, settings[row.key], view())).toBe(true);
    expect(rowDisabled(row, settings, view())).toBe(true);
    expect(await setServerSetting(row.key, true, ipc)).toBe(false);
    expect(ipc.write).not.toHaveBeenCalled();
  });

  test("the three settings the runtime reports as imposed are locked, each by its own flag", () => {
    // GIVEN free settings in the file
    const settings = get(serverSettings);
    const fixable: [ServerSettingKey, keyof LocalServerView["imposedByPolicy"]][] = [
      ["server.serve_on_lan", "serveOnLan"],
      ["server.require_auth", "requireAuth"],
      ["server.allow_request_mcp", "allowRequestMcp"],
    ];

    for (const [key, flag] of fixable) {
      // WHEN the state of the server says the policy imposes one of them
      const free = { serveOnLan: false, requireAuth: false, allowRequestMcp: false };
      const state = view({ imposedByPolicy: { ...free, [flag]: true } });
      const row = SERVER_SETTING_ROWS.find((item) => item.key === key)!;

      // THEN that row is locked and the two others stay free
      expect(rowDisabled(row, settings, state)).toBe(true);
      const others = fixable.filter(([other]) => other !== key);
      for (const [other] of others) expect(imposedByPolicy(other, settings[other], state)).toBe(false);
      // AND nothing is locked without a policy
      expect(imposedByPolicy(key, settings[key], view())).toBe(false);
    }
  });

  test("a locked row shows the mention of the policy, in both languages", () => {
    // GIVEN the source of the row and of the panel
    const row = source("ServerSettingRow.svelte");
    const panel = source("ServerSettingsPanel.svelte");

    // WHEN what a locked row draws is read
    // THEN the mention is the one every setting shares, under the name
    expect(row).toContain("{#if locked}");
    expect(row).toContain("SERVER_SETTINGS_TEXTS.policyLocked");
    expect(panel).toContain("{locked}");
    expect(panel).toContain("{disabled}");
    expect(SERVER_SETTINGS_TEXTS.policyLocked).toBe("settings.policy_locked");
    expect(text(en, SERVER_SETTINGS_TEXTS.policyLocked)).toMatch(/policy/i);
    expect(text(fr, SERVER_SETTINGS_TEXTS.policyLocked)).not.toBe(text(en, SERVER_SETTINGS_TEXTS.policyLocked));
  });

  test("the delay takes no value while auto-unload is off", async () => {
    // GIVEN auto-unload turned off
    const { ipc } = memoryShell();
    await setServerSetting(SERVER_SETTING_KEYS.jitAutoUnload, false, ipc);
    const delay = SERVER_SETTING_ROWS.find((item) => item.control === "minutes")!;

    // WHEN the delay row is asked whether it can be set
    // THEN it cannot, though no policy imposes it
    expect(rowDisabled(delay, get(serverSettings), view())).toBe(true);
    expect(imposedByPolicy(delay.key, get(serverSettings)[delay.key], view())).toBe(false);
  });
});

describe("the server settings: a port that cannot be used", () => {
  test("only a whole number a person may choose is a port", () => {
    // GIVEN what a person may type
    // WHEN each is read as a port
    // THEN the range is kept and the rest refused
    expect(parsePort("7772")).toBe(7772);
    expect(parsePort(` ${LOWEST_PORT} `)).toBe(1024);
    expect(parsePort(String(HIGHEST_PORT))).toBe(65535);
    for (const wrong of ["", "abc", "80", "1023", "65536", "70000", "77.72", "-1", "7772a"]) {
      expect(parsePort(wrong)).toBeNull();
    }
  });

  test("an invalid port is said and never written", async () => {
    // GIVEN a running server on 7772
    const { ipc } = memoryShell();
    const { all, restart } = deps(ipc, { running: true });

    // WHEN something that is not a port is typed
    const done = await changePort("99999", 7772, all);

    // THEN the failure names it, nothing is written, the server untouched
    expect(done).toEqual({ failure: { kind: "port_invalid" } });
    expect(ipc.write).not.toHaveBeenCalled();
    expect(restart).not.toHaveBeenCalled();
    expect(get(serverSettings)["server.port"].value).toBe(7772);
  });

  test("a port another program holds is said with its number, the server read again", async () => {
    // GIVEN a running server, and a port another program holds
    const { ipc } = memoryShell();
    const { all, read } = deps(ipc, { running: true, taken: true });

    // WHEN that port is typed
    const done = await changePort("8080", 7772, all);

    // THEN the failure names the port typed, and the state read after it
    // says the server stopped
    expect(done.failure).toEqual({ kind: "port_taken", port: 8080 });
    expect(read).toHaveBeenCalledTimes(1);
    expect(done.view?.running).toBe(false);
    // AND the bar says it too, under itself
    expect(barFailure(done.failure)).toEqual({ kind: "port_in_use", port: 8080 });
  });

  test("a reopening that fails otherwise is said in the runtime's words", async () => {
    // GIVEN a running server whose reopening fails
    const { ipc } = memoryShell();
    const { all } = deps(ipc, { running: true, broken: true });

    // WHEN the port changes
    const done = await changePort("8080", 7772, all);

    // THEN the failure carries the reason
    expect(done.failure).toEqual({ kind: "reopen", message: "failed to listen" });
    expect(barFailure(done.failure)).toEqual({ kind: "other", message: "failed to listen" });
  });

  test("the port in force typed again writes nothing", async () => {
    // GIVEN the port in force
    const { ipc } = memoryShell();
    const { all } = deps(ipc, { running: true });

    // WHEN it is typed again
    const done = await changePort("7772", 7772, all);

    // THEN nothing happens
    expect(done).toEqual({ failure: null });
    expect(ipc.write).not.toHaveBeenCalled();
  });

  test("both errors are readable in both languages, and the panel stays open", () => {
    // GIVEN the texts of the two errors and the source of the panel
    const panel = source("ServerSettingsPanel.svelte");

    // WHEN they are read
    // THEN each language says what to type, and the taken port is named
    for (const catalogue of [fr, en]) {
      expect(text(catalogue, SERVER_SETTINGS_TEXTS.portInvalid)).toMatch(/1024.*65535/);
      expect(text(catalogue, SERVER_SETTINGS_TEXTS.portTaken)).toContain("{port}");
    }
    expect(panel).toContain('data-testid="developer-server-port-invalid"');
    expect(panel).toContain('data-testid="developer-server-port-taken"');
    // AND nothing in the panel closes it: only the person does
    expect(panel).not.toMatch(/open\s*=\s*false/);
  });

  test("a delay that is not a whole number of minutes is said and never written", async () => {
    // GIVEN the delay in force
    const { ipc } = memoryShell();
    const { all } = deps(ipc);

    // WHEN a fraction is typed, then a whole number
    const wrong = await changeMinutes("1.5", 60, all);
    const right = await changeMinutes("15", 60, all);

    // THEN the first is refused before any write, the second written
    expect(wrong).toEqual({ failure: { kind: "minutes_invalid" } });
    expect(right.failure).toBeNull();
    expect(ipc.write).toHaveBeenCalledTimes(1);
    expect(ipc.write).toHaveBeenCalledWith("server.jit_idle_ttl_minutes", 15);
  });
});

describe("the server settings and Settings > Developer show the same keys", () => {
  const shared = [...ON_DEMAND_ROWS.map((row) => row.key), SEPARATE_REASONING_ROW.key];

  test("the five settings of Settings > Developer are rows of the panel, on the same key", () => {
    // GIVEN the rows of Settings > Developer
    // WHEN they are looked for among the rows of the panel
    const here = SERVER_SETTING_ROWS.map((row) => row.key);

    // THEN each is there: loading, auto-unload, its delay, the last only, the reasoning
    expect(shared).toHaveLength(5);
    for (const key of shared) expect(here).toContain(key);
  });

  test("a value changed in the panel is read by Settings > Developer, and the reverse", async () => {
    // GIVEN one apollia.toml behind the shell, read by both screens
    const { file, ipc } = memoryShell();
    const { all } = deps(ipc);

    for (const key of shared) {
      // WHEN the panel changes the value
      const fromPanel = otherValue(key);
      await changeServerSetting(key, fromPanel, all);

      // THEN the file holds it under its one key, and Settings > Developer,
      // which hydrates the store from the shell, reads it
      expect(file.get(key)).toBe(fromPanel);
      await hydrateServerSettings(ipc);
      expect(get(serverSettings)[key]).toEqual({ value: fromPanel, source: "user" });

      // WHEN Settings > Developer writes the same key back
      await setServerSetting(key, SERVER_SETTING_DEFAULTS[key], ipc);

      // THEN the panel, opened again, reads it
      await hydrateServerSettings(ipc);
      expect(get(serverSettings)[key].value).toBe(SERVER_SETTING_DEFAULTS[key]);
    }
    // AND one key each was ever written
    const written = new Set(vi.mocked(ipc.write).mock.calls.map(([key]) => key));
    expect([...written].sort()).toEqual([...shared].sort());
  });

  test("the panel reads the shell each time it opens", () => {
    // GIVEN the source of the panel
    const panel = source("ServerSettingsPanel.svelte");

    // WHEN its opening is read
    // THEN it hydrates the shared store, so a value written by the settings
    // window is the one shown
    expect(panel).toMatch(/if \(!open\) return;[\s\S]*hydrateServerSettings\(\)/);
  });
});

describe("the developer screen has no file of server declarations", () => {
  test("no source of the screen names such a file", () => {
    // GIVEN every source of the developer screen, and the name of the file,
    // assembled here so that this test is not itself a place that spells it
    const files = readdirSync(HERE).filter((name) => !name.endsWith(".test.ts"));
    expect(files).toContain("ServerSettingsPanel.svelte");
    const fileName = new RegExp(["mcp", "json"].join("\\."), "i");
    expect(fileName.test(["MCP", "json"].join("."))).toBe(true);

    // WHEN each is searched for the file name
    const naming = files.filter((name) => fileName.test(source(name)));

    // THEN none names it: connections and accesses stand in its place
    expect(naming).toEqual([]);
  });
});
