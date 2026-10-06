/**
 * Typed Tauri command wrappers for the local server of the developer screen:
 * its state, its start and its stop
 * (`crates/apollia-desktop/src/commands/local_server.rs`).
 */
import { invoke } from "@tauri-apps/api/core";

/** The local server as the runtime reports it. */
export interface LocalServerView {
  /** Whether the server listens. */
  running: boolean;
  /** The port it listens on, or the configured port when it is stopped. */
  port: number;
  /** Whether it is reachable from the local network. */
  serveOnLan: boolean;
  /** The address a client copies. */
  address: string;
  /** Why the last start failed, `null` after a start that succeeded. */
  lastError: string | null;
}

/** What `startLocalServer` rejects with. */
export interface LocalServerRefusal {
  /** `port_in_use` when another program holds the port. */
  kind: "port_in_use" | "other";
  /** The runtime's own words. */
  message: string;
}

/** Reads the state of the local server. */
export function getLocalServer(): Promise<LocalServerView> {
  return invoke<LocalServerView>("get_local_server");
}

/** Starts the local server; rejects with a `LocalServerRefusal`. */
export function startLocalServer(): Promise<LocalServerView> {
  return invoke<LocalServerView>("start_local_server");
}

/** Stops the local server. */
export function stopLocalServer(): Promise<LocalServerView> {
  return invoke<LocalServerView>("stop_local_server");
}
