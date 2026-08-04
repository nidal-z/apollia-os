/**
 * Lifetime guard for subscriptions registered asynchronously.
 *
 * `listen()` resolves after a round-trip to the Rust side. A component that
 * awaits anything before subscribing, loading its session for instance, can be
 * destroyed while that round-trip is in flight. Its teardown then runs before
 * the handle exists, the assignment lands on a dead component, and the
 * subscription stays live for the rest of the process: it keeps deserializing
 * every event and keeps mutating state nobody renders.
 *
 * The guard makes the order irrelevant. Registration after disposal releases
 * the subscription on the spot, so no handle can outlive the guard whatever
 * sequence the event loop produces.
 */

/** A subscription handle, as returned by `listen()` or a store's `subscribe()`. */
export type Unsubscribe = () => void;

/** Tracks subscriptions and releases them exactly once. */
export interface SubscriptionGuard {
  /**
   * Take ownership of a subscription. Returns the handle when the guard is
   * still live, or releases it immediately and returns `undefined` when
   * disposal already happened.
   */
  keep(unsubscribe: Unsubscribe): Unsubscribe | undefined;
  /** Release everything held, and everything registered afterwards. */
  dispose(): void;
  /** True once `dispose` has run. */
  readonly disposed: boolean;
}

/** Create a guard for one component's subscriptions. */
export function createSubscriptionGuard(): SubscriptionGuard {
  let disposed = false;
  const held: Unsubscribe[] = [];

  return {
    keep(unsubscribe: Unsubscribe): Unsubscribe | undefined {
      if (disposed) {
        unsubscribe();
        return undefined;
      }
      held.push(unsubscribe);
      return unsubscribe;
    },
    dispose(): void {
      disposed = true;
      // Splice as we go so a double dispose cannot release a handle twice.
      while (held.length > 0) {
        held.pop()?.();
      }
    },
    get disposed(): boolean {
      return disposed;
    },
  };
}
