/**
 * The search of the conversation list.
 *
 * Titles are filtered in this window, as they always were. What was said is
 * searched by the runtime, which answers conversations with an excerpt. The
 * two meet here: the conversations found by their title first, then the ones
 * found only by their messages, each under the passage that matched.
 */
import { writable } from "svelte/store";
import { searchChatMessages, type MessageSearchHit } from "$lib/ipc/chat";

/** Shortest query the runtime is asked: one letter matches almost everything. */
export const MESSAGE_SEARCH_MIN_CHARS = 2;

/** Pause after the last keystroke before the runtime is asked. */
export const MESSAGE_SEARCH_DELAY_MS = 200;

/** Whether `query` is long enough to search the messages. */
export function searchesMessages(query: string): boolean {
  return query.trim().length >= MESSAGE_SEARCH_MIN_CHARS;
}

/** A conversation found by its messages, with the passage that matched. */
export interface MessageMatch<S> {
  session: S;
  excerpt: string;
}

/** What a search shows: the title matches, then the message matches. */
export interface SearchSections<S> {
  titles: S[];
  messages: MessageMatch<S>[];
}

/**
 * Lay out the answer of a search. `titleMatches` are the conversations the
 * local filter kept; `hits` are the ones the runtime found in the messages.
 * A conversation found both ways is shown once, under the titles. A hit on a
 * conversation the list does not show (archived in this window, or removed
 * since) is dropped.
 */
export function searchSections<S extends { id: string }>(
  titleMatches: readonly S[],
  hits: readonly MessageSearchHit[],
  listed: readonly S[],
): SearchSections<S> {
  const byTitle = new Set(titleMatches.map((session) => session.id));
  const byId = new Map(listed.map((session) => [session.id, session]));
  const messages: MessageMatch<S>[] = [];
  for (const hit of hits) {
    if (byTitle.has(hit.session_id)) continue;
    const session = byId.get(hit.session_id);
    if (session) messages.push({ session, excerpt: hit.excerpt });
  }
  return { titles: [...titleMatches], messages };
}

/** The state of the search in the messages, for the query it answers. */
export interface MessageSearchState {
  query: string;
  hits: MessageSearchHit[];
  failed: boolean;
}

export const NO_MESSAGE_SEARCH: MessageSearchState = { query: "", hits: [], failed: false };

/**
 * A searcher that asks the runtime once typing pauses, and gives only the
 * answer of the last query: an answer that arrives after a newer query was
 * typed is dropped, so a slow search never replaces a recent one.
 */
export function createMessageSearcher(
  onstate: (state: MessageSearchState) => void,
  delayMs: number = MESSAGE_SEARCH_DELAY_MS,
): { search: (query: string) => void; cancel: () => void } {
  let timer: ReturnType<typeof setTimeout> | null = null;
  let latest = 0;

  function cancel(): void {
    latest += 1;
    if (timer !== null) clearTimeout(timer);
    timer = null;
  }

  function search(rawQuery: string): void {
    cancel();
    const query = rawQuery.trim();
    if (!searchesMessages(query)) {
      onstate(NO_MESSAGE_SEARCH);
      return;
    }
    const ticket = latest;
    timer = setTimeout(() => {
      timer = null;
      searchChatMessages(query).then(
        (hits) => {
          if (ticket === latest) onstate({ query, hits, failed: false });
        },
        (err: unknown) => {
          console.warn("search_chat_messages failed:", err);
          if (ticket === latest) onstate({ query, hits: [], failed: true });
        },
      );
    }, delayMs);
  }

  return { search, cancel };
}

/** The search in the messages the conversation list shows. */
export const messageSearch = writable<MessageSearchState>(NO_MESSAGE_SEARCH);

const listSearcher = createMessageSearcher((state) => messageSearch.set(state));

/** Search the messages for what is typed in the list. */
export const searchMessagesFor = listSearcher.search;

/** Forget the search in the messages, and any answer still on its way. */
export function clearMessageSearch(): void {
  listSearcher.cancel();
  messageSearch.set(NO_MESSAGE_SEARCH);
}

/** The hits to show for `query`: none while the answer is for an older one. */
export function hitsFor(state: MessageSearchState, query: string): MessageSearchHit[] {
  return state.query === query.trim() ? state.hits : [];
}
