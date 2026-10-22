import { describe, expect, test } from "vitest";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";
import type {
  JournalEntryDetail,
  JournalLine,
  JournalQuery,
  JournalSummary,
} from "$lib/ipc/journal";
import {
  DEFAULT_PERIOD,
  NO_FILTER,
  PAGE_SIZE,
  STATE_LABEL,
  activeFilterCount,
  anyUnfolded,
  detailView,
  expansionFor,
  fetchPage,
  flip,
  foldAll,
  groupByDay,
  hasCallDetail,
  iconOf,
  isUnfolded,
  kindTag,
  lineKey,
  metaOf,
  natureOptions,
  ownDetail,
  queryOf,
  sentenceOf,
  stateOf,
  technicalView,
  visibleLines,
  type JournalIpc,
} from "./journal";

const NOW = new Date(2026, 9, 2, 15, 0, 0);

function line(
  kind: JournalLine["kind"],
  summary: Partial<JournalSummary> = {},
  payload: Record<string, unknown> = {},
  at: Partial<Pick<JournalLine, "seq" | "run_id" | "ts">> = {},
): JournalLine {
  const ts = at.ts ?? new Date(2026, 9, 2, 14, 32, 0).toISOString();
  return {
    seq: at.seq ?? 1,
    run_id: at.run_id ?? "run-1",
    ts,
    kind,
    payload,
    prev_hash: "sha256:previous",
    hash: "sha256:this",
    summary: {
      agent: "watch",
      nature: "action",
      family: null,
      what: "",
      tool: null,
      target: null,
      outcome: null,
      ts,
      ...summary,
    },
  };
}

const SECRET = "the quarterly figures are attached";

/** A finished call to send a mail: the target is the recipient, the body is content. */
function sentMail(at: Partial<Pick<JournalLine, "seq" | "run_id" | "ts">> = {}): JournalLine {
  return line(
    "tool_call_completed",
    {
      agent: "mail sorter",
      family: "send_external",
      tool: "gmail.send",
      target: "claire.martin@example.com",
      outcome: "success",
      what: "Tool call finished",
    },
    { agent_id: "mail sorter", tool_name: "gmail.send", tool_call_id: "call-1", success: true },
    at,
  );
}

const names = (agent: string | null): string => agent ?? "an agent";

function resolve(catalog: unknown, dotted: string): unknown {
  return dotted
    .split(".")
    .reduce<unknown>(
      (node, part) =>
        typeof node === "object" && node !== null ? (node as Record<string, unknown>)[part] : undefined,
      catalog,
    );
}

describe("journal - one line per entry, in plain language", () => {
  test("a line says who did what to what, when and with which result, without content", () => {
    // GIVEN a finished call that sent a mail
    const entry = sentMail();

    // WHEN the line is read
    const sentence = sentenceOf(entry, names);
    const meta = metaOf(entry);

    // THEN it names the agent, the action by its family, the target and the result
    expect(sentence).toEqual({
      key: "journal.sentence.tool_done_send_external",
      values: { agent: "mail sorter" },
    });
    expect(meta).toEqual({
      target: "claire.martin@example.com",
      categoryKey: "agent_sheet.family_send_external",
    });
    expect(stateOf(entry)).toBe("success");
    expect(iconOf(entry)).toBe("agent");
    expect(resolve(en, sentence.key)).toBe("{agent} sent data outside");
    expect(resolve(fr, sentence.key)).toMatch(/^\{agent\} /);
    expect(resolve(fr, sentence.key)).not.toBe(resolve(en, sentence.key));
  });

  test("the line of an entry holds nothing of the arguments nor of the output", () => {
    // GIVEN the detail of a call whose arguments and output carry content
    const detail: JournalEntryDetail = {
      ...sentMail(),
      tool_call: {
        tool_call_id: "call-1",
        arguments: { to: "claire.martin@example.com", body: SECRET },
        output: { delivered: SECRET },
        status: "success",
      },
    };

    // WHEN everything the line shows is gathered
    const sentence = sentenceOf(detail, names);
    const meta = metaOf(detail);
    const shown = JSON.stringify([sentence, meta, stateOf(detail), iconOf(detail)]);

    // THEN the content is in none of it
    expect(shown).not.toContain(SECRET);
    expect(shown).not.toContain("body");
  });

  test("a failed call and a refused one read as an attempt, with their own state", () => {
    // GIVEN a failed call, a call refused by a rule and one blocked by the policy
    const failed = line("tool_call_completed", { family: "execute_code", outcome: "failure" });
    const refused = line("rule_refused", { family: "write_or_delete", outcome: "refused" });
    const blocked = line("policy_refused", { family: "send_external", outcome: "refused" });

    // WHEN each line is read
    // THEN the failed call says the agent tried, and the three states differ
    expect(sentenceOf(failed, names).key).toBe("journal.sentence.tool_tried_execute_code");
    expect(stateOf(failed)).toBe("failure");
    expect(sentenceOf(refused, names).key).toBe("journal.sentence.rule_refused");
    expect(stateOf(refused)).toBe("refused");
    expect(stateOf(blocked)).toBe("blocked");
  });

  test("an access change reads as granted or added, under its nature", () => {
    // GIVEN an access granted, a standing consent and a connection added
    const granted = line(
      "access_changed",
      { nature: "access_change", outcome: "success", target: "file_read" },
      { event: "access_changed", change: "granted" },
    );
    const revoked = line(
      "access_changed",
      { nature: "access_change", outcome: "success" },
      { event: "access_changed", change: "revoked" },
    );
    const always = line(
      "consent_answered",
      { family: "send_external", outcome: "success" },
      { event: "consent_answered", answer: "always" },
    );
    const added = line(
      "connection_added",
      { agent: null, nature: "access_change", outcome: "success", target: "mcp:drive" },
      { event: "added" },
    );

    // WHEN each line is read
    // THEN the sentence follows the change, and the state is the one of the closed list
    expect(sentenceOf(granted, names).key).toBe("journal.sentence.access_granted");
    expect(stateOf(granted)).toBe("granted");
    expect(metaOf(granted).categoryKey).toBe("journal.nature.access_change");
    expect(sentenceOf(revoked, names).key).toBe("journal.sentence.access_revoked");
    expect(stateOf(revoked)).toBe("success");
    expect(sentenceOf(always, names).key).toBe("journal.sentence.consent_always");
    expect(stateOf(always)).toBe("granted");
    expect(sentenceOf(added, names).key).toBe("journal.sentence.connection_added");
    expect(stateOf(added)).toBe("added");
    expect(iconOf(added)).toBe("connection");
    expect(iconOf(granted)).toBe("access");
  });

  test("a tool given another family reads as a sentence, not as a system line", () => {
    // GIVEN the entry the runtime writes when the operator reclassifies a tool
    const reclassified = line("tool_reclassified", { nature: "access_change" });

    // WHEN its sentence is resolved
    // THEN it has words of its own, in both languages
    expect(sentenceOf(reclassified, names).key).toBe("journal.sentence.tool_reclassified");
    expect(typeof resolve(en, "journal.sentence.tool_reclassified")).toBe("string");
    expect(typeof resolve(fr, "journal.sentence.tool_reclassified")).toBe("string");
  });

  test("every sentence and every state has its words in both languages", () => {
    // GIVEN one line per kind the journal writes for a reader, per family and per ending
    const kinds = [
      "always_withdrawn", "policy_refused", "rule_refused", "paused_refused", "rule_added",
      "rule_removed",
      "batch_access_granted", "agent_installed", "agent_updated", "agent_uninstalled",
      "agent_override_changed", "package_installed", "package_updated", "package_uninstalled",
      "connection_added", "connection_modified", "connection_removed", "tool_reclassified",
      "connection_warning_accepted", "connection_secret_set", "connection_secret_removed",
      "account_connected", "account_disconnected", "folder_added", "policy_applied",
      "policy_changed", "policy_unreadable", "fail_closed_triggered", "journal_exported",
      "journal_purged", "journal_unsigned", "journal_export_failed",
    ];
    const lines = [
      ...kinds.map((kind) => line(kind)),
      ...["granted", "revoked", "rescoped"].map((change) => line("access_changed", {}, { change })),
      ...["once", "always", "refuse"].map((answer) => line("consent_answered", {}, { answer })),
      ...["read", "read_personal", "write_or_delete", "send_external", "execute_code", "unclassified", null]
        .flatMap((family) => [
          line("tool_call_started", { family }),
          line("tool_call_completed", { family, outcome: "success" }),
          line("tool_call_completed", { family, outcome: "failure" }),
        ]),
      line("llm_completion", { nature: "system", what: "Model response kept for replay" }),
      line({ unknown: { raw_kind: "SomethingNew" } }, { nature: "system" }),
    ];

    // WHEN the key of each sentence and of each state is resolved in each catalogue
    const keys = new Set([...lines.map((l) => sentenceOf(l, names).key), ...Object.values(STATE_LABEL)]);

    // THEN each one is a text, in French and in English
    for (const key of keys) {
      expect(typeof resolve(fr, key), `fr ${key}`).toBe("string");
      expect(typeof resolve(en, key), `en ${key}`).toBe("string");
    }
    expect(keys.size).toBeGreaterThan(50);
  });

  test("an entry the journal keeps for itself is told as technical", () => {
    // GIVEN an entry kept for replay and an event the journal has no mapping for
    const kept = line("llm_completion", { nature: "system", what: "Model response kept for replay" });
    const unmapped = line({ unknown: { raw_kind: "SomethingNew" } }, { nature: "system", what: "SomethingNew" });

    // WHEN their sentences are read
    // THEN both go through the developer wording, with the raw kind
    expect(sentenceOf(kept, names)).toEqual({
      key: "dev.journal.sentence_system",
      values: { kind: "llm_completion", what: "Model response kept for replay" },
    });
    expect(kindTag(unmapped.kind)).toBe("SomethingNew");
    expect(iconOf(kept)).toBe("system");
  });

  test("outside the developer mode a call reads as one line once it ended", () => {
    // GIVEN the end and the start of one call, and the start of a call still running
    const ended = sentMail({ seq: 3 });
    const started = line("tool_call_started", {}, { tool_call_id: "call-1" }, { seq: 2 });
    const running = line("tool_call_started", {}, { tool_call_id: "call-2" }, { seq: 4 });
    const otherRun = line("tool_call_started", {}, { tool_call_id: "call-1" }, { run_id: "run-2" });
    const lines = [running, ended, started, otherRun];

    // WHEN the lines to show are picked for each mode
    const reader = visibleLines(lines, false);
    const developer = visibleLines(lines, true);

    // THEN the reader sees the ended call once and the running one; the developer sees all
    expect(reader).toEqual([running, ended, otherRun]);
    expect(developer).toEqual(lines);
  });

  test("the lines are grouped by day, today and yesterday named", () => {
    // GIVEN two entries of today, one of yesterday, one older and one whose date is unreadable
    const lines = [
      sentMail({ seq: 5, ts: new Date(2026, 9, 2, 14, 32).toISOString() }),
      sentMail({ seq: 4, ts: new Date(2026, 9, 2, 9, 0).toISOString() }),
      sentMail({ seq: 3, ts: new Date(2026, 9, 1, 17, 46).toISOString() }),
      sentMail({ seq: 2, ts: new Date(2026, 8, 29, 15, 12).toISOString() }),
      sentMail({ seq: 1, ts: "not a date" }),
    ];

    // WHEN they are grouped
    const groups = groupByDay(lines, NOW);

    // THEN each day is a group, in the order received
    expect(groups.map((g) => [g.day, g.relative, g.lines.length])).toEqual([
      ["2026-10-02", "today", 2],
      ["2026-10-01", "yesterday", 1],
      ["2026-09-29", "other", 1],
      ["", "other", 1],
    ]);
    expect(groups[3].date).toBeNull();
  });
});

describe("journal - see more, and the developer mode", () => {
  test("outside the developer mode every entry arrives folded and one click unfolds it", () => {
    // GIVEN the expansion a reader starts with
    const start = expansionFor(false);

    // WHEN one entry is turned
    const turned = flip(start, "run-1#1");

    // THEN nothing was unfolded at first, and only that entry is afterwards
    expect(isUnfolded(start, "run-1#1")).toBe(false);
    expect(anyUnfolded(start, ["run-1#1", "run-1#2"])).toBe(false);
    expect(isUnfolded(turned, "run-1#1")).toBe(true);
    expect(isUnfolded(turned, "run-1#2")).toBe(false);
  });

  test("in developer mode every entry arrives unfolded, and all fold at once", () => {
    // GIVEN the expansion the developer mode starts with
    const start = expansionFor(true);

    // WHEN one entry is folded, then all of them, then one is unfolded again
    const oneFolded = flip(start, "run-1#1");
    const allFolded = foldAll(false);
    const oneBack = flip(allFolded, "run-1#2");

    // THEN the detail was shown by default, and each gesture is kept
    expect(isUnfolded(start, "run-1#1")).toBe(true);
    expect(isUnfolded(start, "anything")).toBe(true);
    expect(isUnfolded(oneFolded, "run-1#1")).toBe(false);
    expect(isUnfolded(oneFolded, "run-1#2")).toBe(true);
    expect(anyUnfolded(allFolded, ["run-1#1", "run-1#2"])).toBe(false);
    expect(isUnfolded(oneBack, "run-1#2")).toBe(true);
    expect(isUnfolded(flip(oneBack, "run-1#2"), "run-1#2")).toBe(false);
  });

  test("the detail of a call shows its target, its arguments and its output in full", () => {
    // GIVEN the detail of a call that read a file
    const detail: JournalEntryDetail = {
      ...line(
        "tool_call_completed",
        { family: "read", tool: "file_read", target: "notes/plan.md", outcome: "success" },
        { tool_call_id: "call-7", tool_name: "file_read" },
      ),
      tool_call: {
        tool_call_id: "call-7",
        arguments: { path: "notes/plan.md", offset: 0, lines: [1, 20] },
        output: { content: "the whole plan" },
        status: "success",
      },
    };

    // WHEN what "see more" shows is built
    const view = detailView(detail);

    // THEN the target is there, each argument by its name, and the output whole
    expect(view.isCall).toBe(true);
    expect(view.target).toBe("notes/plan.md");
    expect(view.tool).toBe("file_read");
    expect(view.familyKey).toBe("agent_sheet.family_read");
    expect(view.state).toBe("success");
    expect(view.parameters).toEqual([
      { name: "path", value: "notes/plan.md" },
      { name: "offset", value: "0" },
      { name: "lines", value: "[1,20]" },
    ]);
    expect(view.output).toBe('{"content":"the whole plan"}');
    expect(view.record).toEqual([]);
  });

  test("a call whose arguments the journal did not keep says so by an absence", () => {
    // GIVEN the detail of a call no model response carries
    const detail: JournalEntryDetail = {
      ...sentMail(),
      tool_call: { tool_call_id: "call-1", arguments: null, output: null, status: null },
    };

    // WHEN the view is built
    const view = detailView(detail);

    // THEN the parameters and the output are absent rather than empty
    expect(view.parameters).toBeNull();
    expect(view.output).toBeNull();
  });

  test("an entry that is about no call shows its record, and needs no second read", () => {
    // GIVEN an access granted by the user
    const granted = line(
      "access_changed",
      { nature: "access_change", outcome: "success", target: "file_read" },
      { event: "access_changed", agent_id: "watch", access_id: "file_read", change: "granted", actor: "user" },
    );

    // WHEN its detail is taken from the line itself
    const own = ownDetail(granted);

    // THEN it is ready at once, and the record lists its fields
    expect(hasCallDetail(granted)).toBe(false);
    expect(hasCallDetail(sentMail())).toBe(true);
    expect(own.status).toBe("ready");
    if (own.status !== "ready") return;
    expect(detailView(own.detail).record).toEqual([
      { name: "agent_id", value: "watch" },
      { name: "access_id", value: "file_read" },
      { name: "change", value: "granted" },
      { name: "actor", value: "user" },
    ]);
  });

  test("the developer mode adds the identifiers, the raw arguments and the hashes", () => {
    // GIVEN the detail of a call
    const detail: JournalEntryDetail = {
      ...sentMail({ seq: 12, run_id: "run-8f21" }),
      tool_call: { tool_call_id: "call-1", arguments: { to: "a@b.c" }, output: null, status: "success" },
    };

    // WHEN the technical view is built
    const technical = technicalView(detail);

    // THEN it carries what the reader view leaves out
    expect(technical).toMatchObject({
      kind: "tool_call_completed",
      tool: "gmail.send",
      seq: 12,
      runId: "run-8f21",
      callId: "call-1",
      hash: "sha256:this",
      prevHash: "sha256:previous",
    });
    expect(JSON.parse(technical.arguments)).toEqual({ to: "a@b.c" });
    expect(lineKey(detail)).toBe("run-8f21#12");
  });
});

describe("journal - the filters are applied by the route", () => {
  function recordingIpc(pages: JournalLine[][]): { ipc: JournalIpc; asked: JournalQuery[] } {
    const asked: JournalQuery[] = [];
    const ipc: JournalIpc = {
      list: async (query) => {
        asked.push(query);
        return pages[asked.length - 1] ?? [];
      },
      entry: async () => null,
    };
    return { ipc, asked };
  }

  test("each of the five filters travels in the query of the route", () => {
    // GIVEN the five filters set
    const filters = {
      agent: "mail sorter",
      family: "send_external",
      nature: "action" as const,
      result: "refused" as const,
      period: "today" as const,
      connection: null,
    };

    // WHEN the query of the first page is written
    const query = queryOf(filters, false, NOW, 0);

    // THEN agent, family, nature, result and period are each a criterion of the route
    expect(query).toEqual({
      agents: ["mail sorter"],
      families: ["send_external"],
      natures: ["action"],
      results: ["refused"],
      since: new Date(2026, 9, 2).toISOString(),
      limit: PAGE_SIZE,
      offset: 0,
    });
  });

  test("without a nature the reader asks for what concerns him, the developer for everything", () => {
    // GIVEN no filter
    // WHEN the query is written for each mode
    const reader = queryOf(NO_FILTER, false, NOW, 0);
    const developer = queryOf(NO_FILTER, true, NOW, 0);

    // THEN the reader leaves the journal's own bookkeeping out, by the route
    expect(reader.natures).toEqual(["action", "access_change", "policy"]);
    expect(developer.natures).toEqual([]);
    expect(reader.agents).toBeUndefined();
    expect(reader.families).toBeUndefined();
    expect(reader.results).toBeUndefined();
    expect(reader.since).toBe(new Date(2026, 8, 25).toISOString());
    expect(natureOptions(false)).not.toContain("system");
    expect(natureOptions(true)).toContain("system");
  });

  test("a technical nature kept from the developer mode is not asked for a reader", () => {
    // GIVEN the nature filter left on the technical entries when the mode goes off
    const filters = { ...NO_FILTER, nature: "system" as const };

    // WHEN the query is written for a reader
    const query = queryOf(filters, false, NOW, 0);

    // THEN the route is asked for the reader natures only
    expect(query.natures).toEqual(["action", "access_change", "policy"]);
  });

  test("the whole journal sets no bound, and the periods count from local midnight", () => {
    // GIVEN the periods of the filter
    // WHEN each start is computed
    // THEN "all" has none and the others start at midnight, days back
    expect(queryOf({ ...NO_FILTER, period: "all" }, false, NOW, 0).since).toBeUndefined();
    expect(queryOf({ ...NO_FILTER, period: "30d" }, false, NOW, 0).since).toBe(
      new Date(2026, 8, 2).toISOString(),
    );
    expect(DEFAULT_PERIOD).toBe("7d");
  });

  test("the screen shows what the route returns and filters nothing itself", async () => {
    // GIVEN a route that answers a line of another agent than the one filtered on
    const stray = sentMail();
    const { ipc, asked } = recordingIpc([[stray]]);
    const filters = { ...NO_FILTER, agent: "watch", result: "failure" as const };

    // WHEN the page is read
    const page = await fetchPage(ipc, filters, false, NOW, 0);

    // THEN the criteria went to the route, and its answer is shown as it came
    expect(asked).toHaveLength(1);
    expect(asked[0].agents).toEqual(["watch"]);
    expect(asked[0].results).toEqual(["failure"]);
    expect(page.lines).toEqual([stray]);
    expect(page.hasMore).toBe(false);
  });

  test("an older page is asked from the count already read, and a full page promises more", async () => {
    // GIVEN a route whose first page is full
    const full = Array.from({ length: PAGE_SIZE }, (_, seq) => sentMail({ seq }));
    const { ipc, asked } = recordingIpc([full, [sentMail({ seq: 99 })]]);

    // WHEN the first page then the next are read
    const first = await fetchPage(ipc, NO_FILTER, false, NOW, 0);
    const next = await fetchPage(ipc, NO_FILTER, false, NOW, first.lines.length);

    // THEN the second query skips what was read, and only the full page promises more
    expect(first.hasMore).toBe(true);
    expect(asked[1].offset).toBe(PAGE_SIZE);
    expect(next.hasMore).toBe(false);
  });

  test("the count of active filters leaves the default period out", () => {
    // GIVEN no filter, then two filters, then another period
    // WHEN they are counted
    // THEN the default period is not a filter, another one is
    expect(activeFilterCount(NO_FILTER)).toBe(0);
    expect(activeFilterCount({ ...NO_FILTER, agent: "watch", result: "refused" })).toBe(2);
    expect(activeFilterCount({ ...NO_FILTER, period: "all" })).toBe(1);
  });
});
