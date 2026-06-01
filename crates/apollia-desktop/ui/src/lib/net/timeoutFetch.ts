/**
 * `timeoutFetch` - a thin wrapper over the Fetch API that aborts the request
 * after a configurable timeout (default 10 000 ms) using an `AbortController`.
 *
 * Used by onboarding flows (and elsewhere) to avoid indefinite hangs on
 * flaky networks. Throws a typed `TimeoutError` so callers can discriminate
 * a timeout from a generic network failure.
 *
 * Story:.
 */

/** Default timeout in milliseconds applied when no `ms` argument is given. */
export const DEFAULT_TIMEOUT_MS = 10_000;

/** Thrown by {@link timeoutFetch} when the request exceeds the timeout budget. */
export class TimeoutError extends Error {
  /** Duration in ms after which the request was aborted. */
  public readonly timeoutMs: number;

  constructor(timeoutMs: number, message?: string) {
    super(message ?? `Request timed out after ${timeoutMs} ms`);
    this.name = "TimeoutError";
    this.timeoutMs = timeoutMs;
  }
}

/**
 * Perform a `fetch` with an abort-based timeout.
 *
 * - Resolves with the `Response` if it arrives before `ms` elapses.
 * - Rejects with {@link TimeoutError} if the timeout fires first.
 * - If the caller supplies its own `signal` in `opts`, we compose abort
 *   reasons - whichever aborts first wins.
 */
export async function timeoutFetch(
  url: string,
  opts: RequestInit = {},
  ms: number = DEFAULT_TIMEOUT_MS,
): Promise<Response> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), ms);

  // Compose the user-provided signal with our internal controller.
  if (opts.signal !== undefined && opts.signal !== null) {
    if (opts.signal.aborted) {
      clearTimeout(timer);
      controller.abort();
    } else {
      opts.signal.addEventListener("abort", () => controller.abort(), {
        once: true,
      });
    }
  }

  try {
    const response = await fetch(url, { ...opts, signal: controller.signal });
    return response;
  } catch (err: unknown) {
    if (controller.signal.aborted) {
      // Distinguish user-abort vs our timeout: only ours is a TimeoutError.
      if (opts.signal?.aborted === true) {
        throw err;
      }
      throw new TimeoutError(ms);
    }
    throw err;
  } finally {
    clearTimeout(timer);
  }
}
