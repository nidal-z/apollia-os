import { beforeAll, beforeEach, describe, expect, test, vi } from "vitest";
import { render } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";
import "$lib/i18n";

// Stores read `localStorage` at import time; the node environment has none.
vi.hoisted(() => {
  const memory = new Map<string, string>();
  globalThis.localStorage = {
    getItem: (k: string) => memory.get(k) ?? null,
    setItem: (k: string, v: string) => void memory.set(k, v),
    removeItem: (k: string) => void memory.delete(k),
    clear: () => memory.clear(),
    key: (i: number) => Array.from(memory.keys())[i] ?? null,
    get length() {
      return memory.size;
    },
  };
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => []) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import ConversationList, { relativeTime, rowState } from "./ConversationList.svelte";
import ConversationFolderRow, { folderDeleteNoteKey } from "./ConversationFolderRow.svelte";
import { offersMove } from "$lib/components/operator/ConversationRow.svelte";
import { chatFolders, collapsedFolderIds } from "$lib/stores/chatFolders";
import { chatTokenCount } from "$lib/stores/chatTokenCount";
import { NO_MESSAGE_SEARCH, messageSearch } from "$lib/stores/chatSearch";
import type { DecoratedSession } from "$lib/stores/chatSessions";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";

function lookup(catalogue: unknown, key: string): unknown {
  return key.split(".").reduce<unknown>(
    (node, part) => (node && typeof node === "object" ? (node as Record<string, unknown>)[part] : undefined),
    catalogue,
  );
}

function session(id: string, title: string): DecoratedSession {
  return {
    id,
    mode: "libre",
    agent_name: null,
    status: "active",
    last_message_preview: null,
    message_count: 2,
    created_at: "2026-10-01T08:00:00Z",
    closed_at: null,
    title,
    pinned: false,
    archived: false,
    unread: false,
    lastActivityAt: "2026-10-01T08:00:00Z",
  };
}

const MARCH = session("s-march", "March invoices");
const VAT = session("s-vat", "Quarterly VAT");
const QUOTES = session("s-quotes", "Quotes follow-up");
const ALL = [MARCH, VAT, QUOTES];

function list(props: { sessions?: DecoratedSession[]; query?: string } = {}): string {
  return render(ConversationList, {
    props: {
      sessions: props.sessions ?? ALL,
      listed: ALL,
      query: props.query ?? "",
      selectedSessionId: null,
      density: "md",
      onselect: () => {},
      onrename: () => {},
      ondelete: () => {},
      onconfigure: () => {},
    },
  }).body;
}

/** The markup from the first `data-testid` named to the next one named. */
function between(html: string, from: string, to: string): string {
  const start = html.indexOf(`data-testid="${from}"`);
  const end = html.indexOf(`data-testid="${to}"`, start + 1);
  return html.slice(start, end === -1 ? undefined : end);
}

beforeAll(async () => {
  locale.set("en");
  await waitLocale("en");
});

beforeEach(() => {
  chatFolders.set([
    { id: "f-accounts", name: "Accounting", position: 0, session_ids: ["s-march", "s-vat"] },
    { id: "f-clients", name: "Clients", position: 1, session_ids: [] },
  ]);
  collapsedFolderIds.set(new Set());
  messageSearch.set(NO_MESSAGE_SEARCH);
});

describe("conversation list - token count", () => {
  test("off, no conversation shows its tokens; on, each shows the tokens generated for it", () => {
    // GIVEN a conversation the model generated 1840 tokens for
    const heavy = { ...QUOTES, output_tokens: 1840 };

    // WHEN the list renders with the preference off, then on
    chatTokenCount.set(false);
    const off = list({ sessions: [heavy] });
    chatTokenCount.set(true);
    const on = list({ sessions: [heavy, MARCH] });
    chatTokenCount.set(false);

    // THEN off shows no count, and on shows the count of each conversation,
    // zero for one the runtime gave none for
    expect(off).not.toContain('data-testid="conversation-row-tokens"');
    expect(on.split('data-testid="conversation-row-tokens"')).toHaveLength(3);
    expect(on).toContain("1.8K tokens");
    expect(on).toContain("0 tokens");
  });
});

describe("conversation list - folders", () => {
  test("the list opens on New folder, then each folder with its conversations, then the rest", () => {
    // GIVEN two folders, the first holding two of the three conversations
    // WHEN the list renders
    const html = list();

    // THEN New folder comes first, the first folder counts and shows its two
    // conversations, the second is empty and says so, the third conversation
    // is out of any folder
    expect(html.indexOf('data-testid="chat-new-folder-btn"')).toBeGreaterThan(-1);
    expect(html.indexOf('data-testid="chat-new-folder-btn"')).toBeLessThan(html.indexOf("Accounting"));
    const accounting = between(html, "chat-folder-sessions", "chat-folder-row");
    expect(accounting).toContain("March invoices");
    expect(accounting).toContain("Quarterly VAT");
    expect(accounting).not.toContain("Quotes follow-up");
    expect(html).toContain("No conversation in this folder.");
    const loose = html.slice(html.indexOf('data-testid="chat-loose-sessions"'));
    expect(loose).toContain("Quotes follow-up");
    expect(loose).not.toContain("March invoices");
  });

  test("a folded folder keeps its name and its count, and hides its conversations", () => {
    // GIVEN the first folder folded
    collapsedFolderIds.set(new Set(["f-accounts"]));

    // WHEN the list renders
    const html = list();

    // THEN the folder is shown folded with its count of two, its
    // conversations are not in the markup, and the loose one still is
    expect(html).toContain("Accounting");
    expect(html).toContain('aria-expanded="false"');
    expect(html).not.toContain("March invoices");
    expect(html).not.toContain("Quarterly VAT");
    expect(html).toContain("Quotes follow-up");
  });

  test("a folder row shows its name and how many conversations it holds", () => {
    // GIVEN a folder of three conversations, unfolded
    // WHEN its row renders
    const html = render(ConversationFolderRow, {
      props: {
        folderId: "f-1",
        name: "Accounting",
        count: 3,
        collapsed: false,
        ontoggle: () => {},
        onrename: () => {},
        ondelete: () => {},
        ondropsession: () => {},
      },
    }).body;

    // THEN the name, the count and the unfolded state are in it
    expect(html).toContain("Accounting");
    expect(between(html, "chat-folder-count", "chat-folder-menu-button")).toContain("3");
    expect(html).toContain('aria-expanded="true"');
    expect(html).toContain('data-folder-id="f-1"');
  });

  test("every conversation can be dragged while the list is not searched", () => {
    // GIVEN the list, then the same list under a search
    // WHEN each renders
    const plain = list();
    const searched = list({ sessions: [MARCH], query: "march" });

    // THEN the rows are draggable in the first and not in the second
    expect(plain).toContain('draggable="true"');
    expect(searched).not.toContain('draggable="true"');
  });

  test("with no conversation the list still offers New folder and says it is empty", () => {
    // GIVEN no conversation and no folder
    chatFolders.set([]);

    // WHEN the list renders
    const html = list({ sessions: [] });

    // THEN New folder is offered and the empty list is said
    expect(html).toContain('data-testid="chat-new-folder-btn"');
    expect(html).toContain("No conversations");
  });

  test("the menu of a folder renames and deletes, and says what happens to its conversations", () => {
    // GIVEN the catalogue of the folder menu
    // WHEN its labels are read in both languages
    // THEN they are the words of the mockup, and the note under Delete says
    // no conversation is deleted, with the count
    expect(lookup(fr, "chat.folders.new")).toBe("Nouveau dossier");
    expect(lookup(en, "chat.folders.new")).toBe("New folder");
    expect(lookup(fr, "chat.folders.rename")).toBe("Renommer le dossier");
    expect(lookup(fr, "chat.folders.delete")).toBe("Supprimer le dossier");
    expect(lookup(fr, folderDeleteNoteKey(3))).toContain("{count}");
    expect(lookup(fr, folderDeleteNoteKey(3))).not.toBe(lookup(en, folderDeleteNoteKey(3)));
    expect(lookup(en, folderDeleteNoteKey(3))).toBe(
      "Its {count} conversations go back out of any folder; none is deleted.",
    );
    expect(folderDeleteNoteKey(1)).toBe("chat.folders.delete_note_one");
    expect(folderDeleteNoteKey(0)).toBe("chat.folders.delete_note_empty");
    for (const count of [0, 1, 3]) {
      expect(lookup(fr, folderDeleteNoteKey(count))).toBeTypeOf("string");
      expect(lookup(en, folderDeleteNoteKey(count))).toBeTypeOf("string");
    }
  });

  test("Move to is offered once a folder exists, and names where a conversation can go", () => {
    // GIVEN a row that can be moved, with and without folders, and one that cannot
    // WHEN each is asked whether it offers Move to
    // THEN only the movable row with a folder does, under the words of the mockup
    expect(offersMove(true, [{ id: "f-1", name: "Clients" }])).toBe(true);
    expect(offersMove(true, [])).toBe(false);
    expect(offersMove(false, [{ id: "f-1", name: "Clients" }])).toBe(false);
    expect(lookup(fr, "chat.folders.move_to")).toBeTypeOf("string");
    expect(lookup(fr, "chat.folders.move_to")).not.toBe(lookup(en, "chat.folders.move_to"));
    expect(lookup(fr, "chat.folders.no_folder")).toBeTypeOf("string");
    expect(lookup(fr, "chat.folders.no_folder")).not.toBe(lookup(en, "chat.folders.no_folder"));
    expect(lookup(en, "chat.folders.move_to")).toBe("Move to");
    expect(lookup(en, "chat.folders.no_folder")).toBe("No folder");
  });
});

describe("conversation list - search", () => {
  test("a search shows the title matches, then the message matches under their excerpt", () => {
    // GIVEN the query "durand": the title filter kept one conversation, and
    // the runtime found another one in its messages
    messageSearch.set({
      query: "durand",
      failed: false,
      hits: [
        {
          session_id: "s-march",
          title: "March invoices",
          agent_name: null,
          created_at: "2026-10-01T08:00:00Z",
          excerpt: "... the highest is the one of Durand (4,200) ...",
        },
      ],
    });

    // WHEN the list renders under that query
    const html = list({ sessions: [QUOTES], query: "durand" });

    // THEN Titles holds the first, In the messages holds the second with its
    // passage, and neither the folders nor New folder are shown
    const titles = between(html, "chat-search-titles", "chat-search-messages");
    const messages = html.slice(html.indexOf('data-testid="chat-search-messages"'));
    expect(titles).toContain("Titles");
    expect(titles).toContain("Quotes follow-up");
    expect(titles).not.toContain("March invoices");
    expect(messages).toContain("In the messages");
    expect(messages).toContain("March invoices");
    expect(between(messages, "conversation-row-excerpt", "none")).toContain(
      "... the highest is the one of Durand (4,200) ...",
    );
    expect(html).not.toContain('data-testid="chat-new-folder-btn"');
    expect(html).not.toContain('data-testid="chat-folder-row"');
  });

  test("the hits of an older query are not shown while the new one is searched", () => {
    // GIVEN the answer of "dur" still held while "durand" is typed, and no
    // title match
    messageSearch.set({
      query: "dur",
      failed: false,
      hits: [
        {
          session_id: "s-march",
          title: null,
          agent_name: null,
          created_at: "2026-10-01T08:00:00Z",
          excerpt: "a durable goods order",
        },
      ],
    });

    // WHEN the list renders under the new query
    const html = list({ sessions: [], query: "durand" });

    // THEN the old passage is not shown, and the list says it is searching
    expect(html).not.toContain("a durable goods order");
    expect(html).toContain("Searching the messages...");
  });

  test("a search that finds nothing says so, and a failed one says it failed", () => {
    // GIVEN the answer of "zzz": nothing, then a failure
    messageSearch.set({ query: "zzz", hits: [], failed: false });
    const nothing = list({ sessions: [], query: "zzz" });
    messageSearch.set({ query: "zzz", hits: [], failed: true });
    const failed = list({ sessions: [], query: "zzz" });

    // WHEN each renders
    // THEN the first says no result, the second that the search failed
    expect(nothing).toContain("No results");
    expect(failed).toContain("The search in the messages did not go through.");
    expect(lookup(fr, "chat.search.titles")).toBe("Titres");
    expect(lookup(fr, "chat.search.in_messages")).toBeTypeOf("string");
    expect(lookup(fr, "chat.search.in_messages")).not.toBe(lookup(en, "chat.search.in_messages"));
    expect(lookup(fr, "chat.search_placeholder")).toBeTypeOf("string");
    expect(lookup(fr, "chat.search_placeholder")).not.toBe(lookup(en, "chat.search_placeholder"));
  });
});

describe("conversation list - a row", () => {
  test("the selected row is active whatever else it is", () => {
    // GIVEN a closed, unread conversation
    const closed = { ...MARCH, status: "closed" as const, unread: true };

    // WHEN its state is read selected and not selected
    // THEN selected wins, then closed
    expect(rowState(closed, true)).toBe("active");
    expect(rowState(closed, false)).toBe("closed");
    expect(rowState({ ...MARCH, unread: true }, false)).toBe("unread");
  });

  test("the age of a conversation is short: seconds, minutes, hours, days", () => {
    // GIVEN a fixed present
    const now = new Date("2026-10-05T12:00:00Z").getTime();

    // WHEN ages of 30 seconds, 5 minutes, 3 hours and 2 days are read
    // THEN each takes its short form, and no date reads as a dash
    expect(relativeTime("2026-10-05T11:59:30Z", now)).toBe("30s");
    expect(relativeTime("2026-10-05T11:55:00Z", now)).toBe("5min");
    expect(relativeTime("2026-10-05T09:00:00Z", now)).toBe("3h");
    expect(relativeTime("2026-10-03T12:00:00Z", now)).toBe("2j");
    expect(relativeTime(null, now)).toBe("-");
    expect(relativeTime("not a date", now)).toBe("-");
  });
});
