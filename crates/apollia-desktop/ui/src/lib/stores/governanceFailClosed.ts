/**
 * Whether governance fails closed, for the banner of the window.
 *
 * Read at start, again on each change the runtime announces
 * (`GovernanceFailClosed` on the `runtime-event` channel), and when the user
 * retries from the banner.
 */
import { writable, type Readable } from "svelte/store";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { governanceState, type FailClosed } from "$lib/ipc/governance";

const state = writable<FailClosed | null>(null);

/** Why governance fails closed, `null` while it decides normally. */
export const governanceFailClosed: Readable<FailClosed | null> = {
  subscribe: state.subscribe,
};

/**
 * Reads the state again. Rejects when the runtime cannot answer, leaving the
 * last state shown.
 */
export async function refreshGovernanceFailClosed(): Promise<void> {
  const current = await governanceState();
  state.set(current.fail_closed);
}

/** Whether a `runtime-event` envelope announces a change of the state. */
export function announcesFailClosedChange(eventType: string): boolean {
  return eventType === "GovernanceFailClosed";
}

/** Starts following the state. Returns the function that stops it. */
export function startGovernanceFailClosedMonitor(): () => void {
  let stopped = false;
  let unlisten: UnlistenFn | null = null;
  void refreshGovernanceFailClosed().catch(() => {});
  void listen<{ event_type: string }>("runtime-event", (event) => {
    if (stopped || !announcesFailClosedChange(event.payload.event_type)) return;
    void refreshGovernanceFailClosed().catch(() => {});
  }).then((fn) => {
    if (stopped) fn();
    else unlisten = fn;
  });
  return () => {
    stopped = true;
    unlisten?.();
    unlisten = null;
  };
}
