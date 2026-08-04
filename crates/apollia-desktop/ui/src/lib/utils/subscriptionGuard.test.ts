import { describe, it, expect, vi } from "vitest";
import { createSubscriptionGuard } from "./subscriptionGuard";

describe("createSubscriptionGuard", () => {
  it("releases a subscription that resolves after disposal", async () => {
    // GIVEN a guard whose owner is torn down while a subscription is still
    // being established, the shape a component gets when it awaits an IPC
    // round-trip before calling listen()
    const guard = createSubscriptionGuard();
    const unsubscribe = vi.fn();
    const pending = new Promise<() => void>((resolve) => {
      setTimeout(() => resolve(unsubscribe), 0);
    });

    guard.dispose();

    // WHEN the subscription finally resolves and is handed to the guard
    const handle = guard.keep(await pending);

    // THEN it is released instead of outliving its owner
    expect(unsubscribe).toHaveBeenCalledTimes(1);
    expect(handle).toBeUndefined();
  });

  it("releases every subscription held at disposal", () => {
    // GIVEN a guard holding several subscriptions
    const guard = createSubscriptionGuard();
    const first = vi.fn();
    const second = vi.fn();
    guard.keep(first);
    guard.keep(second);

    // WHEN the owner is torn down
    guard.dispose();

    // THEN all of them are released
    expect(first).toHaveBeenCalledTimes(1);
    expect(second).toHaveBeenCalledTimes(1);
    expect(guard.disposed).toBe(true);
  });

  it("never releases the same subscription twice", () => {
    // GIVEN a guard already disposed once
    const guard = createSubscriptionGuard();
    const unsubscribe = vi.fn();
    guard.keep(unsubscribe);
    guard.dispose();

    // WHEN disposal runs again
    guard.dispose();

    // THEN the subscription is not released a second time
    expect(unsubscribe).toHaveBeenCalledTimes(1);
  });

  it("hands the handle back while the owner is alive", () => {
    // GIVEN a live guard
    const guard = createSubscriptionGuard();
    const unsubscribe = vi.fn();

    // WHEN a subscription is registered
    const handle = guard.keep(unsubscribe);

    // THEN it is returned untouched and stays subscribed
    expect(handle).toBe(unsubscribe);
    expect(unsubscribe).not.toHaveBeenCalled();
    expect(guard.disposed).toBe(false);
  });
});
