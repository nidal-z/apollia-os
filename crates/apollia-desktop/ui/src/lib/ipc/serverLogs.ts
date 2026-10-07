/**
 * Typed Tauri command wrappers for the logs of the local server: their
 * stream, their clearing, and the window they detach into
 * (`crates/apollia-desktop/src/commands/server_logs.rs`,
 * `crates/apollia-desktop/src/commands/logs_window.rs`).
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** The lowest level a stream sends. */
export type ServerLogLevel = "debug" | "info" | "warn" | "error";

/** One line of the logs, as the runtime records it. */
export interface ServerLogLine {
  /** Position of the line since the runtime started. */
  seq: number;
  /** When the line was recorded, RFC 3339 in UTC. */
  timestamp: string;
  level: ServerLogLevel;
  /** A request the server answered, or a line a local engine printed. */
  source: "request" | "engine";
  method?: string | null;
  endpoint?: string | null;
  model?: string | null;
  status?: number | null;
  durationMs?: number | null;
  message?: string | null;
}

/** The lines one chunk of a stream carried, for the watch that asked. */
export interface ServerLogBatch {
  watch: string;
  lines: ServerLogLine[];
}

const LINES_EVENT = "server-logs:lines";
const CLOSED_EVENT = "server-logs:closed";
const REATTACHED_EVENT = "server-logs:reattached";

/**
 * Streams the logs at `level` and above to this window: what the runtime
 * holds, then each new line. The route applies the level. Asking again ends
 * the stream before; the lines arrive through `listenServerLogLines`.
 */
export function watchServerLogs(watch: string, level: ServerLogLevel): Promise<void> {
  return invoke<void>("watch_server_logs", { watch, level });
}

/** Ends the stream this window reads. */
export function unwatchServerLogs(): Promise<void> {
  return invoke<void>("unwatch_server_logs");
}

/** Empties the logs of the server; answers how many lines they held. */
export function clearServerLogs(): Promise<number> {
  return invoke<number>("clear_server_logs");
}

/** Opens the logs in a window of their own, or brings it to the front. */
export function openServerLogsWindow(): Promise<"created" | "focused"> {
  return invoke<"created" | "focused">("open_server_logs_window");
}

/** Closes the logs window; answers whether one was open. */
export function closeServerLogsWindow(): Promise<boolean> {
  return invoke<boolean>("close_server_logs_window");
}

/** Subscribes to the lines of the streams; returns the unlisten handle. */
export function listenServerLogLines(
  handler: (batch: ServerLogBatch) => void,
): Promise<UnlistenFn> {
  return listen<ServerLogBatch>(LINES_EVENT, (event) => handler(event.payload));
}

/** Subscribes to a stream ending without having been asked to. */
export function listenServerLogsClosed(handler: (watch: string) => void): Promise<UnlistenFn> {
  return listen<{ watch: string }>(CLOSED_EVENT, (event) => handler(event.payload.watch));
}

/** Subscribes to the logs window being closed. */
export function listenServerLogsReattached(handler: () => void): Promise<UnlistenFn> {
  return listen<null>(REATTACHED_EVENT, () => handler());
}
