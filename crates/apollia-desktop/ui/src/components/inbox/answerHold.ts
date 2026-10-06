/**
 * An answer given on the "To review" screen is held for a short time before it
 * is sent, so its line can offer "Cancel" and mean it: a decision already
 * applied cannot be taken back, a conversation that resumed has resumed.
 *
 * Held answers are sent when their delay runs out, and all at once when the
 * screen is left, so leaving never loses an answer.
 */

/** The clock the hold runs on; replaced in tests. */
export interface HoldTimers {
  set: (run: () => void, delayMs: number) => unknown;
  clear: (handle: unknown) => void;
}

const REAL_TIMERS: HoldTimers = {
  set: (run, delayMs) => setTimeout(run, delayMs),
  clear: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
};

export interface AnswerHold<R> {
  /** Holds `request` for the item `id`; a second answer replaces the first. */
  hold: (id: string, request: R) => void;
  /** Takes the held answer of `id` back. False when it was already sent. */
  cancel: (id: string) => boolean;
  /** Sends every held answer now. */
  flush: () => void;
  /** Whether an answer of `id` is still held. */
  holds: (id: string) => boolean;
}

/**
 * `send` applies one answer; `settled` is told once it has, with the error
 * when it failed, so the screen can put the item back.
 */
export function createAnswerHold<R>(options: {
  delayMs: number;
  send: (id: string, request: R) => Promise<void>;
  settled: (id: string, error: unknown) => void;
  timers?: HoldTimers;
}): AnswerHold<R> {
  const timers = options.timers ?? REAL_TIMERS;
  const held = new Map<string, { handle: unknown; request: R }>();

  function release(id: string): void {
    const entry = held.get(id);
    if (!entry) return;
    held.delete(id);
    timers.clear(entry.handle);
    options.send(id, entry.request).then(
      () => options.settled(id, null),
      (error: unknown) => options.settled(id, error ?? new Error("the answer was not sent")),
    );
  }

  return {
    hold(id, request) {
      const previous = held.get(id);
      if (previous) timers.clear(previous.handle);
      const handle = timers.set(() => release(id), options.delayMs);
      held.set(id, { handle, request });
    },
    cancel(id) {
      const entry = held.get(id);
      if (!entry) return false;
      timers.clear(entry.handle);
      held.delete(id);
      return true;
    },
    flush() {
      for (const id of [...held.keys()]) release(id);
    },
    holds: (id) => held.has(id),
  };
}
