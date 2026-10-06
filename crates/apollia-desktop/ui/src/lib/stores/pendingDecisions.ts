/**
 * The decisions waiting for the user, as the runtime lists them: one list,
 * read by the review screen and counted by the sidebar. The count of what
 * waits is the length of that list and nothing else, so the number beside the
 * entry and the lines of the screen cannot disagree.
 *
 * The list is read again when a store fed by the live events moves, and on a
 * slow tick for what no event announces (a batch item put on hold).
 */
import { derived, writable, type Readable } from "svelte/store";
import { listPendingDecisions, type PendingDecision } from "$lib/ipc/pendingDecisions";
import { pendingApprovals } from "$lib/stores/hitl";
import { pendingChatApprovals, pendingUserInputs } from "$lib/stores/chat";

/** How often the list is read again with no event to announce a change. */
export const PENDING_REFRESH_MS = 15_000;

const items = writable<PendingDecision[]>([]);

/** Every decision waiting for the user, as last listed by the runtime. */
export const pendingDecisions: Readable<PendingDecision[]> = { subscribe: items.subscribe };

/** How many decisions wait for the user: the one count of the interface. */
export const toReviewCount: Readable<number> = derived(items, ($items) => $items.length);

let latestRead = 0;

/**
 * Reads the list again and publishes it. Rejects with the runtime error, the
 * last list published staying in place; an answer that arrives after a later
 * read was started is dropped, so a slow read never overwrites a fresh one.
 */
export async function refreshPendingDecisions(): Promise<PendingDecision[]> {
  const read = ++latestRead;
  const listed = await listPendingDecisions();
  if (read === latestRead) items.set(listed);
  return listed;
}

/**
 * Keeps the list fresh while something shows its count. Returns the function
 * that stops the watch.
 */
export function watchPendingDecisions(): () => void {
  // The sidebar has no place to show a failed read; the review screen reads
  // the same list itself and shows the error there.
  const read = () => void refreshPendingDecisions().catch(() => {});
  const stops = [pendingApprovals, pendingChatApprovals, pendingUserInputs].map((store) =>
    store.subscribe(read),
  );
  const tick = setInterval(read, PENDING_REFRESH_MS);
  return () => {
    clearInterval(tick);
    for (const stop of stops) stop();
  };
}
