import { describe, it, expect } from "vitest";
import { createIdentityGuard } from "./identityGuard";

describe("createIdentityGuard", () => {
  it("keeps the ticket of a request aimed at what is on screen", () => {
    // GIVEN a panel showing one item and loading it
    let selected = "alpha";
    const guard = createIdentityGuard(() => selected);

    // WHEN the request is opened and the selection does not move
    const ticket = guard.begin();

    // THEN its result may be written
    expect(ticket.current).toBe(true);
  });

  it("refuses a response whose identity is no longer the selection", () => {
    // GIVEN a panel that starts loading "alpha"
    let selected = "alpha";
    const guard = createIdentityGuard(() => selected);
    const ticket = guard.begin();

    // WHEN the operator picks "beta" before that response comes back
    selected = "beta";

    // THEN the late response of "alpha" is refused
    expect(ticket.current).toBe(false);
  });

  it("refuses it even when no second request follows the selection", () => {
    // GIVEN a sheet loading "alpha", the shape of a panel whose selection can
    // move with no fetch behind it
    let selected = "alpha";
    const guard = createIdentityGuard(() => selected);
    const ticket = guard.begin();

    // WHEN the selection moves and nothing else is started
    selected = "beta";

    // THEN the response in flight is still refused
    expect(ticket.current).toBe(false);
  });

  it("takes a response back once the selection returns to its identity", () => {
    // GIVEN a request for "alpha" opened before two selection changes
    let selected = "alpha";
    const guard = createIdentityGuard(() => selected);
    const ticket = guard.begin();

    // WHEN the operator leaves "alpha" and comes back to it
    selected = "beta";
    const whileAway = ticket.current;
    selected = "alpha";

    // THEN the response is refused while away and accepted on return, since it
    // carries the data of what is on screen
    expect(whileAway).toBe(false);
    expect(ticket.current).toBe(true);
  });

  it("leaves two requests aimed at the same identity both current", () => {
    // GIVEN two loaders of the same item overlapping, the shape the chat has
    // when a refresh and a finalisation run on one session
    const guard = createIdentityGuard(() => "alpha");

    // WHEN both open a ticket
    const first = guard.begin();
    const second = guard.begin();

    // THEN neither strands the other
    expect(first.current).toBe(true);
    expect(second.current).toBe(true);
  });

  it("gives each component its own guard", () => {
    // GIVEN two panels reading two different selections
    let left = "alpha";
    const leftGuard = createIdentityGuard(() => left);
    const rightGuard = createIdentityGuard(() => "gamma");
    const leftTicket = leftGuard.begin();
    const rightTicket = rightGuard.begin();

    // WHEN the left panel changes selection
    left = "beta";

    // THEN only the left panel's request is refused
    expect(leftTicket.current).toBe(false);
    expect(rightTicket.current).toBe(true);
  });

  it("paints the second selection and never the first, on the loader shape the panels use", async () => {
    // GIVEN the loader the five panels share: read by identity, then write. The
    // first read is slower than the second, the interleaving an operator
    // produces by clicking a second item while the first is still loading.
    let selected = "alpha";
    let painted: string | null = null;
    let loading = false;
    const guard = createIdentityGuard(() => selected);
    const latency: Record<string, number> = { alpha: 30, beta: 1 };

    async function load(id: string): Promise<void> {
      const ticket = guard.begin();
      loading = true;
      try {
        const data = await new Promise<string>((resolve) => {
          setTimeout(() => resolve(`data of ${id}`), latency[id]);
        });
        if (!ticket.current) return;
        painted = data;
      } finally {
        if (ticket.current) loading = false;
      }
    }

    // WHEN "alpha" is selected, then "beta" before "alpha" answers
    const alpha = load("alpha");
    selected = "beta";
    const beta = load("beta");
    await beta;

    // THEN "beta" is on screen as soon as it answers
    expect(painted).toBe("data of beta");
    expect(loading).toBe(false);

    // AND the late "alpha" answer neither replaces it nor re-arms the spinner
    await alpha;
    expect(painted).toBe("data of beta");
    expect(loading).toBe(false);
  });
});
