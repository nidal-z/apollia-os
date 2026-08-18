/**
 * Identity guard for responses that come back after the selection moved.
 *
 * A panel that reads its data from the selected item writes the response
 * whenever it resolves, and nothing compares that response to what is selected
 * by then. Picking a second item while the first is still loading therefore
 * paints the first item's data under the second item's name, and it stays there
 * until the next selection. `createSubscriptionGuard` does not cover this: its
 * contract is `keep` / `dispose`, it knows about teardown and not about
 * identity.
 *
 * This guard makes the arrival order irrelevant. A component declares how to
 * read the identity it currently shows, opens a ticket for each request it
 * starts, and writes only while that ticket is current. The ticket stops being
 * current the moment the identity it was aimed at stops being the one on
 * screen, whether or not a fresh request follows: a selection can move with no
 * new fetch behind it, a sheet that closes for instance, and the response in
 * flight must still write nothing.
 *
 * Two requests aimed at the same identity are both current, deliberately. The
 * guard arbitrates between identities, never between requests: making the older
 * one stale would strand the flags of any loader that overlaps another on the
 * same item, and both of them carry that item's data anyway.
 *
 * Identities are compared with `===`, so use a value type: a name, an id.
 */

/** Reads the identity the component currently shows. */
export type CurrentIdentity<Id> = () => Id;

/** One request in flight, and whether its result may still be written. */
export interface RequestTicket {
  /** True while the request is still aimed at the identity on screen. */
  readonly current: boolean;
}

/** Hands out tickets for one component's requests. */
export interface IdentityGuard {
  /**
   * Open a ticket for a request about to start, capturing the identity that
   * request is aimed at.
   */
  begin(): RequestTicket;
}

/** Create a guard for one component's requests. */
export function createIdentityGuard<Id>(
  currentIdentity: CurrentIdentity<Id>,
): IdentityGuard {
  return {
    begin(): RequestTicket {
      const aimedAt = currentIdentity();
      return {
        get current(): boolean {
          return aimedAt === currentIdentity();
        },
      };
    },
  };
}
