/**
 * Runtime heartbeat + reconnect store.
 *
 * The Tauri backend emits `runtime:heartbeat` events at a regular cadence.
 * If no heartbeat arrives within `HEARTBEAT_TIMEOUT_MS`, the runtime is
 * marked as disconnected and an exponential-backoff reconnect loop kicks
 * in - attempting `refreshAll()` until a fresh heartbeat (or any runtime
 * event) is observed.
 *
 * Consumers subscribe to `runtimeHealth` to render the persistent
 * `RuntimeDisconnectedBanner`.
 */
import { writable, get } from "svelte/store";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { refreshAll } from "./sse";

export type RuntimeHealthStatus = "connected" | "disconnected" | "reconnecting";

export interface RuntimeHealth {
  status: RuntimeHealthStatus;
  /** Milliseconds since the last heartbeat, or `null` before the first one. */
  lastHeartbeatAt: number | null;
  /** Current reconnect attempt (0 when healthy). */
  attempt: number;
  /** ISO timestamp of the next scheduled retry (diagnostic / UX). */
  nextRetryAt: string | null;
}

/** Grace window before we mark the runtime disconnected. */
const HEARTBEAT_TIMEOUT_MS = 15_000;
/** Initial retry delay - doubles on every failure up to `MAX_BACKOFF_MS`. */
const INITIAL_BACKOFF_MS = 1_000;
const MAX_BACKOFF_MS = 30_000;

export const runtimeHealth = writable<RuntimeHealth>({
  status: "connected",
  lastHeartbeatAt: null,
  attempt: 0,
  nextRetryAt: null,
});

let watchdog: ReturnType<typeof setTimeout> | null = null;
let retryTimer: ReturnType<typeof setTimeout> | null = null;
let unlistenHeartbeat: UnlistenFn | null = null;
let unlistenRuntimeEvent: UnlistenFn | null = null;
let destroyed = false;

function markConnected(): void {
  if (retryTimer !== null) {
    clearTimeout(retryTimer);
    retryTimer = null;
  }
  runtimeHealth.set({
    status: "connected",
    lastHeartbeatAt: Date.now(),
    attempt: 0,
    nextRetryAt: null,
  });
  armWatchdog();
}

function armWatchdog(): void {
  if (watchdog !== null) clearTimeout(watchdog);
  if (destroyed) return;
  watchdog = setTimeout(() => {
    if (destroyed) return;
    onHeartbeatMissed();
  }, HEARTBEAT_TIMEOUT_MS);
}

function onHeartbeatMissed(): void {
  const current = get(runtimeHealth);
  if (current.status === "connected") {
    runtimeHealth.set({
      ...current,
      status: "disconnected",
      attempt: 0,
    });
  }
  scheduleRetry();
}

function scheduleRetry(): void {
  if (destroyed) return;
  const current = get(runtimeHealth);
  const attempt = current.attempt + 1;
  const delay = Math.min(
    INITIAL_BACKOFF_MS * 2 ** (attempt - 1),
    MAX_BACKOFF_MS,
  );
  runtimeHealth.set({
    ...current,
    status: "reconnecting",
    attempt,
    nextRetryAt: new Date(Date.now() + delay).toISOString(),
  });
  if (retryTimer !== null) clearTimeout(retryTimer);
  retryTimer = setTimeout(() => {
    void attemptReconnect();
  }, delay);
}

async function attemptReconnect(): Promise<void> {
  if (destroyed) return;
  try {
    await refreshAll();
    // `refreshAll` returns on fulfilment regardless of subsequent heartbeats,
    // so we keep the watchdog engaged - a real heartbeat (or runtime-event)
    // will flip the store back to `connected`. If nothing arrives, the next
    // heartbeat miss re-enters `scheduleRetry` with an increased attempt.
    armWatchdog();
  } catch {
    scheduleRetry();
  }
}

/** Manually trigger a reconnect - wired to the banner's "Retry now" button. */
export function triggerReconnect(): void {
  if (retryTimer !== null) {
    clearTimeout(retryTimer);
    retryTimer = null;
  }
  void attemptReconnect();
}

/**
 * Start listening for `runtime:heartbeat` events and any runtime event
 * (which also resets the watchdog).  Returns a teardown function.
 */
export function startRuntimeHealthMonitor(): () => void {
  destroyed = false;
  armWatchdog();

  void listen("runtime:heartbeat", () => {
    if (destroyed) return;
    markConnected();
  }).then((fn) => {
    if (destroyed) fn();
    else unlistenHeartbeat = fn;
  });

  // Any runtime event is proof the bridge is alive, even if the explicit
  // heartbeat channel is not wired yet on the backend.
  void listen("runtime-event", () => {
    if (destroyed) return;
    markConnected();
  }).then((fn) => {
    if (destroyed) fn();
    else unlistenRuntimeEvent = fn;
  });

  return () => {
    destroyed = true;
    if (watchdog !== null) {
      clearTimeout(watchdog);
      watchdog = null;
    }
    if (retryTimer !== null) {
      clearTimeout(retryTimer);
      retryTimer = null;
    }
    if (unlistenHeartbeat !== null) {
      unlistenHeartbeat();
      unlistenHeartbeat = null;
    }
    if (unlistenRuntimeEvent !== null) {
      unlistenRuntimeEvent();
      unlistenRuntimeEvent = null;
    }
  };
}

/**
 * Classifies an error message coming from the `get_chat_session` IPC.
 * Backend errors propagate their rusqlite/runtime wording through the Tauri
 * bridge - surface enough to pick the right UI state without locking in
 * exact substrings.
 */
export function classifySessionError(raw: string): "not_found" | "corrupted" | "other" {
  const message = raw.toLowerCase();
  if (message.includes("not found") || message.includes("no row") || message.includes("unknown session")) {
    return "not_found";
  }
  if (
    message.includes("sqlite") ||
    message.includes("database") ||
    message.includes("corrupt") ||
    message.includes("malformed") ||
    message.includes("decode") ||
    message.includes("serde")
  ) {
    return "corrupted";
  }
  return "other";
}
