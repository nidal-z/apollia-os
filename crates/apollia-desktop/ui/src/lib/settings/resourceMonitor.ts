// The resource monitor: the memory, the processor and the video memory the
// machine is using, read while somebody looks at them.
//
// The store measures only while it has a subscriber. The first subscription
// starts the reads, one every `intervalMs` once the previous one has answered;
// the last unsubscription stops them, so a closed screen asks nothing. Every
// surface that shows these figures subscribes to the same store, which keeps
// one reader for all of them.

import { readable, type Readable } from "svelte/store";
import { getResourceUsage, type ResourceUsage } from "$lib/ipc/system";

/** Time between the answer to a measurement and the next one. */
export const RESOURCE_MONITOR_INTERVAL_MS = 2000;

/**
 * A store of the latest measurement, `null` until the first one answers.
 *
 * A failed read keeps the last figures and tries again at the next tick: the
 * monitor is a diagnostic, and a daemon that restarts must not blank it.
 */
export function createResourceMonitor(
  read: () => Promise<ResourceUsage> = getResourceUsage,
  intervalMs: number = RESOURCE_MONITOR_INTERVAL_MS,
): Readable<ResourceUsage | null> {
  return readable<ResourceUsage | null>(null, (set) => {
    let stopped = false;
    let timer: ReturnType<typeof setTimeout> | undefined;

    async function tick(): Promise<void> {
      try {
        const usage = await read();
        if (!stopped && usage) set(usage);
      } catch {
        // Kept silent on purpose: see the function comment.
      }
      if (!stopped) timer = setTimeout(() => void tick(), intervalMs);
    }

    void tick();
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  });
}

/** The measurement every surface reads. */
export const resourceUsage = createResourceMonitor();

/**
 * `monitor` read only while `shown` is true.
 *
 * The returned store subscribes to the monitor when `shown` turns true and
 * unsubscribes when it turns false, so a hidden surface stops the reads the
 * same way a closed screen does: the monitor measures only while it has a
 * subscriber. Hidden, the store holds `null`.
 */
export function whileShown(
  shown: Readable<boolean>,
  monitor: Readable<ResourceUsage | null> = resourceUsage,
): Readable<ResourceUsage | null> {
  return readable<ResourceUsage | null>(null, (set) => {
    let stop: (() => void) | null = null;
    const stopWatching = shown.subscribe((visible) => {
      if (visible && stop === null) {
        stop = monitor.subscribe(set);
      } else if (!visible && stop !== null) {
        stop();
        stop = null;
        set(null);
      }
    });
    return () => {
      stopWatching();
      stop?.();
      stop = null;
    };
  });
}
