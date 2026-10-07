import { describe, test, expect, vi } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const invoke = vi.hoisted(() => vi.fn(async (..._args: unknown[]): Promise<unknown> => null));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));

import { get, writable } from "svelte/store";
import {
  DEFAULT_LOG_LEVEL,
  LEVEL_LABELS,
  LOG_LEVELS,
  appendLines,
  createLogsView,
  followsEnd,
  lineLevel,
  lineText,
  lineTime,
  scrollAfterLines,
  type LogsViewStores,
  type ServerLogsIpc,
} from "./serverLogs";
import {
  clearServerLogs,
  closeServerLogsWindow,
  openServerLogsWindow,
  watchServerLogs,
  type ServerLogBatch,
  type ServerLogLevel,
  type ServerLogLine,
} from "$lib/ipc/serverLogs";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";

const HERE = __dirname;
const RANK: Record<ServerLogLevel, number> = { debug: 0, info: 1, warn: 2, error: 3 };

function request(seq: number, level: ServerLogLevel, status: number): ServerLogLine {
  return {
    seq,
    timestamp: `2026-10-07T09:15:0${seq % 10}.250Z`,
    level,
    source: "request",
    method: "POST",
    endpoint: "/v1/chat/completions",
    model: "qwen3-4b",
    status,
    durationMs: 120 + seq,
    message: null,
  };
}

function engine(seq: number, level: ServerLogLevel, message: string): ServerLogLine {
  return { seq, timestamp: `2026-10-07T09:15:0${seq % 10}.500Z`, level, source: "engine", message };
}

/**
 * The runtime and the shell behind the logs, held in memory: the ring of the
 * server's logs, the route that streams it at a level and the one that
 * empties it, the audit journal beside it, and the window the logs detach
 * into. The stream sends what the ring holds at the level, then each line
 * recorded while a stream is open.
 */
function fakeShell(held: ServerLogLine[] = []) {
  const ring = [...held];
  const journal = ["agent.run task-1", "permission.revoked gmail"];
  const handlers = {
    lines: [] as ((batch: ServerLogBatch) => void)[],
    closed: [] as ((watch: string) => void)[],
    reattached: [] as (() => void)[],
  };
  let stream: { watch: string; level: ServerLogLevel } | null = null;
  let windowOpen = false;
  const subscribe = <T>(list: T[], handler: T) => {
    list.push(handler);
    return () => void list.splice(list.indexOf(handler), 1);
  };
  const send = (lines: ServerLogLine[]) => {
    if (!stream) return;
    const { watch, level } = stream;
    const passing = lines.filter((line) => RANK[line.level] >= RANK[level]);
    if (passing.length > 0) for (const h of [...handlers.lines]) h({ watch, lines: passing });
  };
  const ipc: ServerLogsIpc = {
    watch: vi.fn(async (watch: string, level: ServerLogLevel) => {
      stream = { watch, level };
      send(ring);
    }),
    unwatch: vi.fn(async () => {
      stream = null;
    }),
    clear: vi.fn(async () => ring.splice(0).length),
    openWindow: vi.fn(async () => {
      const outcome = windowOpen ? "focused" : "created";
      windowOpen = true;
      return outcome;
    }),
    closeWindow: vi.fn(async () => {
      if (!windowOpen) return false;
      windowOpen = false;
      for (const h of [...handlers.reattached]) h();
      return true;
    }),
    onLines: vi.fn(async (h) => subscribe(handlers.lines, h)),
    onClosed: vi.fn(async (h) => subscribe(handlers.closed, h)),
    onReattached: vi.fn(async (h) => subscribe(handlers.reattached, h)),
  };
  return {
    ipc,
    ring,
    journal,
    handlers,
    /** A line the server records while streams are open. */
    record(line: ServerLogLine) {
      ring.push(line);
      send([line]);
    },
    /** The window system closing the detached window. */
    closeWindowFromOutside() {
      windowOpen = false;
      for (const h of [...handlers.reattached]) h();
    },
    /** The stream ending on its own. */
    drop() {
      const ended = stream;
      stream = null;
      if (ended) for (const h of [...handlers.closed]) h(ended.watch);
    },
    streaming: () => stream,
    windowOpen: () => windowOpen,
  };
}

function stores(): LogsViewStores {
  return { collapsed: writable(false), detached: writable(false) };
}

const flush = () => new Promise((done) => setTimeout(done, 0));

describe("the logs pane - the stream", () => {
  test("the pane shows the lines held, then each new one, with its time and its level", async () => {
    // GIVEN a server whose logs hold two lines
    const shell = fakeShell([request(1, "info", 200), engine(2, "warn", "slot busy")]);
    const logs = createLogsView(shell.ipc, { stores: stores() });

    // WHEN the pane opens, then the server records a third line
    await logs.open();
    const opened = get(logs);
    shell.record(request(3, "error", 500));
    const after = get(logs);

    // THEN the stream was asked of the shell, and the held lines came first
    expect(shell.ipc.watch).toHaveBeenCalledTimes(1);
    expect(opened.status).toBe("live");
    expect(opened.lines.map((line) => line.seq)).toEqual([1, 2]);
    // AND the new line follows them, in order
    expect(after.lines.map((line) => line.seq)).toEqual([1, 2, 3]);
    // AND each line reads with its time to the millisecond and its level
    const expected = new Date("2026-10-07T09:15:03.250Z");
    expect(lineTime(after.lines[2].timestamp)).toMatch(/^\d{2}:\d{2}:\d{2}\.250$/);
    expect(lineTime(after.lines[2].timestamp).slice(3, 5)).toBe(
      String(expected.getMinutes()).padStart(2, "0"),
    );
    expect(lineLevel(after.lines[2])).toBe("ERROR");
    expect(lineText(after.lines[2])).toBe("POST  /v1/chat/completions  500  123 ms  qwen3-4b");
    expect(lineText(after.lines[1])).toBe("slot busy");
  });

  test("a line of a stream the pane ended is not shown", async () => {
    // GIVEN an open pane
    const shell = fakeShell([request(1, "info", 200)]);
    const logs = createLogsView(shell.ipc, { stores: stores() });
    await logs.open();

    // WHEN a batch arrives under the name of another stream
    for (const h of shell.handlers.lines) h({ watch: "stale", lines: [request(9, "info", 200)] });

    // THEN the pane keeps its own lines only
    expect(get(logs).lines.map((line) => line.seq)).toEqual([1]);
  });

  test("a line sent twice is shown once, and the view keeps the last lines only", () => {
    // GIVEN three lines held
    const held = [request(1, "info", 200), request(2, "info", 200), request(3, "info", 200)];

    // WHEN a batch repeats the last and brings two more, under a cap of four
    const next = appendLines(held, [request(3, "info", 200), request(4, "info", 200), request(5, "info", 200)], 4);

    // THEN the repeat is dropped and the oldest line leaves
    expect(next.map((line) => line.seq)).toEqual([2, 3, 4, 5]);
  });

  test("the view follows the end while the reader stays there", () => {
    // GIVEN a view of 200 pixels over 1000 pixels of lines
    // WHEN it sits at its end, then 300 pixels above it, and lines come in
    const atEnd = followsEnd(800, 200, 1000);
    const nearEnd = followsEnd(790, 200, 1000);
    const above = followsEnd(500, 200, 1000);

    // THEN at its end it follows the new end, above it stays where it was
    expect(atEnd).toBe(true);
    expect(nearEnd).toBe(true);
    expect(above).toBe(false);
    expect(scrollAfterLines(atEnd, 800, 1400)).toBe(1400);
    expect(scrollAfterLines(above, 500, 1400)).toBe(500);
  });

  test("a stream that ends on its own says so, and resumes on demand", async () => {
    // GIVEN an open pane whose stream drops
    const shell = fakeShell([request(1, "info", 200)]);
    const logs = createLogsView(shell.ipc, { stores: stores() });
    await logs.open();
    shell.drop();
    const dropped = get(logs).status;

    // WHEN the reader resumes
    await logs.resume();

    // THEN the pane said the stream was interrupted, then reads it again
    expect(dropped).toBe("interrupted");
    expect(get(logs).status).toBe("live");
    expect(shell.ipc.watch).toHaveBeenCalledTimes(2);
  });

  test("a stream that cannot be opened shows why", async () => {
    // GIVEN a runtime without the server
    const shell = fakeShell();
    shell.ipc.watch = vi.fn(async () => {
      throw new Error("the compatible server is not part of this runtime");
    });
    const logs = createLogsView(shell.ipc, { stores: stores() });

    // WHEN the pane opens
    await logs.open();

    // THEN it is not live and carries the runtime's own words
    expect(get(logs).status).toBe("failed");
    expect(get(logs).error).toBe("the compatible server is not part of this runtime");
  });

  test("closing the pane ends its stream", async () => {
    // GIVEN an open pane
    const shell = fakeShell();
    const logs = createLogsView(shell.ipc, { stores: stores() });
    await logs.open();

    // WHEN the screen leaves it
    await logs.close();

    // THEN the shell is told to stop, and nothing listens any more
    expect(shell.ipc.unwatch).toHaveBeenCalledTimes(1);
    expect(shell.streaming()).toBeNull();
    expect(shell.handlers.lines).toEqual([]);
  });
});

describe("the logs pane - the level filter", () => {
  test("the filter offers the four levels of the route, each with its label in both languages", () => {
    // GIVEN the levels of the filter
    // WHEN their labels are looked up in the two catalogues
    const labels = LOG_LEVELS.map((level) => LEVEL_LABELS[level].split(".").pop() as string);

    // THEN they are the route's four, opened on every line
    expect([...LOG_LEVELS]).toEqual(["debug", "info", "warn", "error"]);
    expect(DEFAULT_LOG_LEVEL).toBe("debug");
    for (const key of labels) {
      expect(fr.dev.developer.logs.levels).toHaveProperty(key);
      expect(en.dev.developer.logs.levels).toHaveProperty(key);
    }
  });

  test("choosing a level asks the route for it, and the pane shows what the route sends", async () => {
    // GIVEN an open pane over logs of three levels
    const shell = fakeShell([
      request(1, "info", 200),
      request(2, "warn", 404),
      request(3, "error", 500),
    ]);
    const logs = createLogsView(shell.ipc, { stores: stores() });
    await logs.open();
    const all = get(logs).lines.length;

    // WHEN the reader chooses warnings and errors
    await logs.setLevel("warn");

    // THEN a new stream was asked at that level
    expect(shell.ipc.watch).toHaveBeenLastCalledWith(expect.any(String), "warn");
    expect(shell.streaming()?.level).toBe("warn");
    // AND the lines shown are those the route sent, the info line gone
    expect(all).toBe(3);
    expect(get(logs).level).toBe("warn");
    expect(get(logs).lines.map((line) => line.seq)).toEqual([2, 3]);
    // AND a line under the level, recorded after, never reaches the pane
    shell.record(request(4, "info", 200));
    shell.record(request(5, "error", 500));
    expect(get(logs).lines.map((line) => line.seq)).toEqual([2, 3, 5]);
  });

  test("the pane filters nothing itself", () => {
    // GIVEN the module that holds the decisions of the pane
    const source = readFileSync(resolve(HERE, "serverLogs.ts"), "utf8");

    // WHEN it is searched for a filter on the level of a line
    const filtersByLevel = /\.filter\([^)]*\.level/.test(source);

    // THEN there is none: the level is the route's to apply
    expect(filtersByLevel).toBe(false);
  });

  test("the wrapper hands the level to the command", async () => {
    // GIVEN the shell
    invoke.mockClear();

    // WHEN a stream is asked at a level
    await watchServerLogs("w-1", "error");

    // THEN the command receives it with the name of the stream
    expect(invoke).toHaveBeenCalledWith("watch_server_logs", { watch: "w-1", level: "error" });
  });
});

describe("the logs pane - clear", () => {
  test("clearing empties the logs of the server and leaves the journal as it was", async () => {
    // GIVEN an open pane over two lines, and a journal of two entries
    const shell = fakeShell([request(1, "info", 200), request(2, "info", 200)]);
    const before = [...shell.journal];
    const logs = createLogsView(shell.ipc, { stores: stores() });
    await logs.open();

    // WHEN the reader clears
    await logs.clear();

    // THEN the route that empties the logs was called, once
    expect(shell.ipc.clear).toHaveBeenCalledTimes(1);
    expect(shell.ring).toEqual([]);
    expect(get(logs).lines).toEqual([]);
    // AND the journal holds what it held
    expect(shell.journal).toEqual(before);
    // AND the stream stays open: the next line shows
    shell.record(request(3, "info", 200));
    expect(get(logs).lines.map((line) => line.seq)).toEqual([3]);
  });

  test("the clear command is the one of the server's logs, and no journal command is called", async () => {
    // GIVEN the shell
    invoke.mockClear();

    // WHEN the logs are cleared
    await clearServerLogs();

    // THEN one command went out, the one bound to the logs route
    expect(invoke.mock.calls.map((call) => call[0])).toEqual(["clear_server_logs"]);
    // AND neither the wrappers nor the pane name a command of the journal
    const sources = ["../../lib/ipc/serverLogs.ts", "serverLogs.ts", "LogsPane.svelte"].map((rel) =>
      readFileSync(resolve(HERE, rel), "utf8"),
    );
    for (const source of sources) {
      expect(source).not.toMatch(/journal["'`(]|ipc\/journal|ipc\/audit/i);
    }
  });

  test("a clearing that fails keeps the lines and says why", async () => {
    // GIVEN an open pane whose runtime refuses to clear
    const shell = fakeShell([request(1, "info", 200)]);
    shell.ipc.clear = vi.fn(async () => {
      throw new Error("the Apollia service does not answer");
    });
    const logs = createLogsView(shell.ipc, { stores: stores() });
    await logs.open();

    // WHEN the reader clears
    await logs.clear();

    // THEN the line is still shown, with the reason
    expect(get(logs).lines.map((line) => line.seq)).toEqual([1]);
    expect(get(logs).error).toBe("the Apollia service does not answer");
  });
});

describe("the logs pane - detach", () => {
  test("detaching opens the window and the screen stops reading the stream", async () => {
    // GIVEN an open pane
    const shell = fakeShell([request(1, "info", 200)]);
    const kept = stores();
    const logs = createLogsView(shell.ipc, { stores: kept });
    await logs.open();

    // WHEN the reader detaches the logs
    await logs.detach();

    // THEN the window is open, the pane says the logs are there, and the
    //      screen reads no stream
    expect(shell.ipc.openWindow).toHaveBeenCalledTimes(1);
    expect(shell.windowOpen()).toBe(true);
    expect(get(logs).detached).toBe(true);
    expect(get(kept.detached)).toBe(true);
    expect(shell.streaming()).toBeNull();
  });

  test("closing the window brings the logs back to the screen", async () => {
    // GIVEN a pane whose logs are detached
    const shell = fakeShell([request(1, "info", 200)]);
    const logs = createLogsView(shell.ipc, { stores: stores() });
    await logs.open();
    await logs.detach();

    // WHEN the window is closed by the window system
    shell.closeWindowFromOutside();
    await flush();

    // THEN the pane shows the logs again and reads their stream
    expect(get(logs).detached).toBe(false);
    expect(get(logs).status).toBe("live");
    expect(get(logs).lines.map((line) => line.seq)).toEqual([1]);
  });

  test("the pane can call the logs back itself", async () => {
    // GIVEN a pane whose logs are detached
    const shell = fakeShell();
    const logs = createLogsView(shell.ipc, { stores: stores() });
    await logs.open();
    await logs.detach();

    // WHEN the reader brings them back from the screen
    await logs.reattach();
    await flush();

    // THEN the window is closed and the pane reads the stream
    expect(shell.ipc.closeWindow).toHaveBeenCalledTimes(1);
    expect(shell.windowOpen()).toBe(false);
    expect(get(logs).detached).toBe(false);
    expect(get(logs).status).toBe("live");
  });

  test("logs said detached with no window left come back at once", async () => {
    // GIVEN a pane that remembers the logs as detached, and no window open
    const shell = fakeShell([request(1, "info", 200)]);
    const kept = stores();
    kept.detached.set(true);
    const logs = createLogsView(shell.ipc, { stores: kept });
    await logs.open();
    const before = get(logs);

    // WHEN the reader brings them back
    await logs.reattach();

    // THEN the pane, which read nothing while detached, reads the stream
    expect(before.detached).toBe(true);
    expect(before.status).toBe("idle");
    expect(get(logs).detached).toBe(false);
    expect(get(logs).lines.map((line) => line.seq)).toEqual([1]);
  });

  test("a window that cannot open leaves the logs on the screen", async () => {
    // GIVEN a window system that refuses
    const shell = fakeShell();
    shell.ipc.openWindow = vi.fn(async () => {
      throw new Error("no display");
    });
    const logs = createLogsView(shell.ipc, { stores: stores() });
    await logs.open();

    // WHEN the reader detaches
    await logs.detach();

    // THEN the pane keeps the logs, reads on, and says why
    expect(get(logs).detached).toBe(false);
    expect(get(logs).status).toBe("live");
    expect(get(logs).error).toBe("no display");
  });

  test("the window reads the logs itself and hands them back by closing", async () => {
    // GIVEN the view of the detached window, the screen having detached
    const shell = fakeShell([request(1, "info", 200)]);
    await shell.ipc.openWindow();
    const windowed = createLogsView(shell.ipc, { windowed: true, stores: stores() });
    await windowed.open();

    // WHEN it is asked to fold or to detach, then to hand the logs back
    windowed.toggleCollapsed();
    await windowed.detach();
    const inWindow = get(windowed);
    await windowed.reattach();

    // THEN it never folds nor detaches, shows the lines, and closing it is
    //      what hands the logs back
    expect(inWindow.collapsed).toBe(false);
    expect(inWindow.detached).toBe(false);
    expect(inWindow.lines.map((line) => line.seq)).toEqual([1]);
    expect(shell.ipc.openWindow).toHaveBeenCalledTimes(1);
    expect(shell.windowOpen()).toBe(false);
  });

  test("the wrappers call the two commands of the window", async () => {
    // GIVEN the shell
    invoke.mockClear();

    // WHEN the window is opened then closed
    await openServerLogsWindow();
    await closeServerLogsWindow();

    // THEN each gesture is its own command
    expect(invoke.mock.calls.map((call) => call[0])).toEqual([
      "open_server_logs_window",
      "close_server_logs_window",
    ]);
  });
});

describe("the logs pane - fold", () => {
  test("the pane folds and unfolds, and remembers it across a remount", async () => {
    // GIVEN an open pane, unfolded
    const shell = fakeShell([request(1, "info", 200)]);
    const kept = stores();
    const logs = createLogsView(shell.ipc, { stores: kept });
    await logs.open();
    const opened = get(logs).collapsed;

    // WHEN the reader folds it, the screen remounts it, then unfolds it
    logs.toggleCollapsed();
    const folded = get(logs).collapsed;
    await logs.close();
    const remounted = createLogsView(shell.ipc, { stores: kept });
    await remounted.open();
    const afterRemount = get(remounted).collapsed;
    remounted.toggleCollapsed();

    // THEN it opened unfolded, folded on demand, stayed folded, and unfolded
    expect(opened).toBe(false);
    expect(folded).toBe(true);
    expect(afterRemount).toBe(true);
    expect(get(remounted).collapsed).toBe(false);
  });

  test("a folded pane keeps counting the lines for its summary", async () => {
    // GIVEN a folded pane
    const shell = fakeShell([request(1, "info", 200)]);
    const logs = createLogsView(shell.ipc, { stores: stores() });
    await logs.open();
    logs.toggleCollapsed();

    // WHEN the server records a line
    shell.record(request(2, "info", 200));

    // THEN the count the summary shows went up
    expect(get(logs).collapsed).toBe(true);
    expect(get(logs).lines.length).toBe(2);
  });

  test("the pane renders its lines only unfolded and on the screen", () => {
    // GIVEN the component of the pane
    const pane = readFileSync(resolve(HERE, "LogsPane.svelte"), "utf8");

    // WHEN its template is read
    // THEN the lines sit behind the fold and the detachment, and the fold
    //      button says which way it goes
    expect(pane).toContain("const shown = $derived(!$logs.collapsed && !$logs.detached);");
    expect(pane).toMatch(/\{#if shown\}[\s\S]*<LogLines lines=\{\$logs\.lines\} \/>/);
    expect(pane).toContain('data-testid="developer-logs-collapse"');
    expect(pane).toContain("aria-expanded={!$logs.collapsed}");
  });
});
