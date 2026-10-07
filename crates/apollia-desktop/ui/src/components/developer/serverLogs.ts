// The logs of the local server, on the developer screen and in the window
// they detach into: the lines held, the level asked of the route, clearing,
// folding, detaching, and when the view follows the end.
//
// The decisions live here, apart from the components, so Vitest can hold them
// without rendering anything.

import { get, writable, type Readable, type Writable } from "svelte/store";
import type { ServerLogBatch, ServerLogLevel, ServerLogLine } from "$lib/ipc/serverLogs";

/** The shell calls the logs make, replaceable in tests. */
export interface ServerLogsIpc {
  watch: (watch: string, level: ServerLogLevel) => Promise<void>;
  unwatch: () => Promise<void>;
  clear: () => Promise<number>;
  openWindow: () => Promise<unknown>;
  closeWindow: () => Promise<unknown>;
  onLines: (handler: (batch: ServerLogBatch) => void) => Promise<() => void>;
  onClosed: (handler: (watch: string) => void) => Promise<() => void>;
  onReattached: (handler: () => void) => Promise<() => void>;
}

/** The levels of the filter, from every line to errors alone. */
export const LOG_LEVELS: readonly ServerLogLevel[] = ["debug", "info", "warn", "error"];

/** The level the logs open on: every line. */
export const DEFAULT_LOG_LEVEL: ServerLogLevel = "debug";

/** The i18n key of each level of the filter. */
export const LEVEL_LABELS: Record<ServerLogLevel, string> = {
  debug: "dev.developer.logs.levels.debug",
  info: "dev.developer.logs.levels.info",
  warn: "dev.developer.logs.levels.warn",
  error: "dev.developer.logs.levels.error",
};

/** How many lines the view keeps, the size of the runtime's own ring. */
export const MAX_LINES = 2000;

/** Where the stream stands. */
export type LogsStatus =
  /** Nothing asked: before the first stream, or while the logs are detached. */
  | "idle"
  /** A stream was asked and has not answered yet. */
  | "connecting"
  /** Lines arrive as they are recorded. */
  | "live"
  /** The stream ended without having been asked to. */
  | "interrupted"
  /** The stream could not be opened. */
  | "failed";

/** What the logs show. */
export interface LogsViewState {
  /** The lines the route sent at the level asked, oldest first. */
  lines: ServerLogLine[];
  /** The level asked of the route. */
  level: ServerLogLevel;
  status: LogsStatus;
  /** Why the stream could not be opened, or the clearing failed. */
  error: string | null;
  /** Whether the pane is folded down to its title and summary. */
  collapsed: boolean;
  /** Whether the logs are shown in their own window instead of the screen. */
  detached: boolean;
}

/** Whether the pane is folded; kept while the screen switches views. */
export const logsCollapsed = writable(false);

/** Whether the logs are in their own window; kept while the screen switches views. */
export const logsDetached = writable(false);

function message(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return String(error);
}

/**
 * `held` followed by the lines of `incoming` it does not hold yet, cut to the
 * last `max`. A line is told by its position: one at or before the last held
 * is a repeat.
 */
export function appendLines(
  held: readonly ServerLogLine[],
  incoming: readonly ServerLogLine[],
  max: number = MAX_LINES,
): ServerLogLine[] {
  let last = held.length > 0 ? held[held.length - 1].seq : -1;
  const next = [...held];
  for (const line of incoming) {
    if (line.seq <= last) continue;
    next.push(line);
    last = line.seq;
  }
  return next.length > max ? next.slice(next.length - max) : next;
}

/** How close to the end, in pixels, a view still counts as following it. */
export const FOLLOW_SLACK = 24;

/** Whether a scrolled view sits at its end, so new lines should keep it there. */
export function followsEnd(scrollTop: number, clientHeight: number, scrollHeight: number): boolean {
  return scrollHeight - (scrollTop + clientHeight) <= FOLLOW_SLACK;
}

/**
 * Where the view scrolls once new lines are in: its end when it was
 * following, where the reader left it otherwise.
 */
export function scrollAfterLines(following: boolean, scrollTop: number, scrollHeight: number): number {
  return following ? scrollHeight : scrollTop;
}

function pad(value: number, width: number): string {
  return String(value).padStart(width, "0");
}

/**
 * The time of a line, to the millisecond, on the clock of this computer.
 * A timestamp that cannot be read is shown as it came.
 */
export function lineTime(timestamp: string): string {
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) return timestamp;
  return `${pad(date.getHours(), 2)}:${pad(date.getMinutes(), 2)}:${pad(date.getSeconds(), 2)}.${pad(date.getMilliseconds(), 3)}`;
}

/** The level of a line as its tag shows it. */
export function lineLevel(line: ServerLogLine): string {
  return line.level.toUpperCase();
}

/**
 * The text of a line: what an engine printed, or the request the server
 * answered (method, path, status, duration, and the model when it named one).
 */
export function lineText(line: ServerLogLine): string {
  if (line.source === "engine") return line.message ?? "";
  const parts = [line.method, line.endpoint];
  if (line.status != null) parts.push(String(line.status));
  if (line.durationMs != null) parts.push(`${line.durationMs} ms`);
  if (line.model) parts.push(line.model);
  if (line.message) parts.push(line.message);
  return parts.filter((part): part is string => Boolean(part)).join("  ");
}

let watches = 0;

/** A name for a stream no other stream of this application bears. */
export function nextWatchId(): string {
  watches += 1;
  return `logs-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}-${watches}`;
}

/** The logs as a component drives them. */
export interface LogsView extends Readable<LogsViewState> {
  /** Subscribes to the shell and opens the stream; call it at mount. */
  open: () => Promise<void>;
  /** Ends the stream and the subscriptions; call it at unmount. */
  close: () => Promise<void>;
  /** Asks the route for another level: the lines shown become what it sends. */
  setLevel: (level: ServerLogLevel) => Promise<void>;
  /** Empties the logs of the server, then the view. */
  clear: () => Promise<void>;
  /** Opens the stream again after it was interrupted or refused. */
  resume: () => Promise<void>;
  /** Folds or unfolds the pane. */
  toggleCollapsed: () => void;
  /** Hands the logs to their own window. */
  detach: () => Promise<void>;
  /** Closes that window, which brings the logs back to the screen. */
  reattach: () => Promise<void>;
}

/** The stores a view keeps its fold and its detachment in. */
export interface LogsViewStores {
  collapsed: Writable<boolean>;
  detached: Writable<boolean>;
}

/**
 * The logs over `ipc`. `windowed` is the view of the detached window: it is
 * never folded nor detached itself, and it reads the stream whatever the
 * screen does.
 */
export function createLogsView(
  ipc: ServerLogsIpc,
  options: { windowed?: boolean; stores?: LogsViewStores; watchId?: () => string } = {},
): LogsView {
  const windowed = options.windowed ?? false;
  const stores = options.stores ?? { collapsed: logsCollapsed, detached: logsDetached };
  const watchId = options.watchId ?? nextWatchId;

  const state = writable<LogsViewState>({
    lines: [],
    level: DEFAULT_LOG_LEVEL,
    status: "idle",
    error: null,
    collapsed: windowed ? false : get(stores.collapsed),
    detached: windowed ? false : get(stores.detached),
  });

  let current: string | null = null;
  let opened = false;
  let unlisten: (() => void)[] = [];

  function patch(change: Partial<LogsViewState>): void {
    state.update((held) => ({ ...held, ...change }));
  }

  async function stream(level: ServerLogLevel): Promise<void> {
    const watch = watchId();
    current = watch;
    patch({ lines: [], level, status: "connecting", error: null });
    try {
      await ipc.watch(watch, level);
      if (current === watch && get(state).status === "connecting") patch({ status: "live" });
    } catch (error) {
      if (current === watch) patch({ status: "failed", error: message(error) });
    }
  }

  async function halt(): Promise<void> {
    current = null;
    patch({ status: "idle" });
    await ipc.unwatch().catch(() => undefined);
  }

  function setDetached(detached: boolean): void {
    stores.detached.set(detached);
    patch({ detached });
  }

  return {
    subscribe: state.subscribe,

    async open() {
      if (opened) return;
      opened = true;
      unlisten = await Promise.all([
        ipc.onLines((batch) => {
          if (batch.watch !== current) return;
          state.update((held) => ({ ...held, lines: appendLines(held.lines, batch.lines) }));
        }),
        ipc.onClosed((watch) => {
          if (watch === current) patch({ status: "interrupted" });
        }),
        ipc.onReattached(() => {
          if (windowed || !get(state).detached) return;
          setDetached(false);
          if (opened) void stream(get(state).level);
        }),
      ]);
      if (!opened) {
        for (const off of unlisten.splice(0)) off();
        return;
      }
      if (!get(state).detached) await stream(get(state).level);
    },

    async close() {
      if (!opened) return;
      opened = false;
      for (const off of unlisten.splice(0)) off();
      await halt();
    },

    async setLevel(level) {
      if (level === get(state).level && get(state).status !== "idle") return;
      if (get(state).detached) {
        patch({ level });
        return;
      }
      await stream(level);
    },

    async clear() {
      try {
        await ipc.clear();
        patch({ lines: [], error: null });
      } catch (error) {
        patch({ error: message(error) });
      }
    },

    async resume() {
      if (get(state).detached) return;
      await stream(get(state).level);
    },

    toggleCollapsed() {
      if (windowed) return;
      const collapsed = !get(state).collapsed;
      stores.collapsed.set(collapsed);
      patch({ collapsed });
    },

    async detach() {
      if (windowed) return;
      try {
        await ipc.openWindow();
      } catch (error) {
        patch({ error: message(error) });
        return;
      }
      setDetached(true);
      await halt();
    },

    async reattach() {
      let closed: unknown;
      try {
        closed = await ipc.closeWindow();
      } catch (error) {
        patch({ error: message(error) });
        return;
      }
      // A window that was open says so itself once it is gone. None was open:
      // nothing will say it, so the logs come back here and now.
      if (closed === false && !windowed && get(state).detached) {
        setDetached(false);
        if (opened) await stream(get(state).level);
      }
    },
  };
}
