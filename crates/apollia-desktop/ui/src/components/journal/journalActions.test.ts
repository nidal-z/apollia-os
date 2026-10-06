import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, test, vi } from "vitest";
import type {
  ExportedJournalEntry,
  JournalExportDocument,
  VerifyJournalReport,
} from "$lib/ipc/journal";
import type { OrgPolicyBody, PolicyView } from "$lib/ipc/policy";
import { NO_FILTER } from "./journal";
import {
  detailFor,
  detailRule,
  exportFileName,
  exportQueryOf,
  filterMentions,
  printRows,
} from "./journalExport";
import { isSound, verdictOf } from "./journalVerify";
import { createJournalActions, type JournalActionsIpc } from "./useJournalActions.svelte";

const here = dirname(fileURLToPath(import.meta.url));
const read = (relative: string): string => readFileSync(resolve(here, relative), "utf8");

function report(over: Partial<VerifyJournalReport> = {}): VerifyJournalReport {
  return {
    ok: true,
    entries_checked: 1284,
    runs_checked: 12,
    first_break: null,
    head_matches_state: true,
    start: null,
    ...over,
  };
}

function policy(detail: "forbid" | "require" | null): PolicyView {
  const body: OrgPolicyBody = {
    schema_version: 1,
    signed_at: "2026-10-01T00:00:00Z",
    connections: { allow: [], deny: [], remote: null },
    families: { deny: [] },
    inference: { allowed_servers: [], remote_endpoints: null, require_model_hash: false },
    settings: {},
    packages: { require_signature: false, trusted_publishers: [] },
    journal: { required: false, retention_days: null, export: { detail, daily_summary_dir: null } },
  };
  return {
    state: "active",
    dir: "/etc/apollia",
    key_fingerprint: "ab12",
    publisher: "IT",
    signed_at: body.signed_at,
    policy: body,
  };
}

function entry(over: Partial<ExportedJournalEntry> = {}): ExportedJournalEntry {
  return {
    run_id: "run-1",
    seq: 7,
    kind: "tool_call_completed",
    hash: "9f2c",
    agent: "mail sorter",
    nature: "action",
    family: "send_external",
    what: "Tool call completed",
    tool: "mail_send",
    target: "client@example.org",
    outcome: "success",
    ts: "2026-10-02T12:32:00Z",
    ...over,
  };
}

function ipcOf(over: Partial<JournalActionsIpc> = {}): JournalActionsIpc {
  const refused = (): Promise<never> => Promise.reject(new Error("not stubbed"));
  return { verify: refused, policy: refused, exportTo: refused, readExport: refused, print: refused, ...over };
}

const named = (agent: string | null): string => agent ?? "An agent";

describe("the verdict of the integrity check", () => {
  test("a journal never purged is intact since its beginning", () => {
    // GIVEN a report of 1284 entries that all hold, with no purge
    // WHEN the verdict is read
    const verdict = verdictOf(report());

    // THEN the journal is intact, counted from its origin
    expect(verdict).toEqual({ kind: "intact", entries: 1284, since: { from: "origin" } });
    expect(isSound(verdict)).toBe(true);
  });

  test("a purged journal is intact since the first entry the purge kept", () => {
    // GIVEN a report whose verification starts after a purge
    const purged = report({
      entries_checked: 300,
      start: { global_seq: 984, ts: "2026-09-12T08:00:00Z", purged_at: "2026-10-01T00:00:00Z" },
    });

    // WHEN the verdict is read
    const verdict = verdictOf(purged);

    // THEN it names the date the verified part begins, not the origin
    expect(verdict).toEqual({
      kind: "intact",
      entries: 300,
      since: { from: "purge", firstEntryAt: "2026-09-12T08:00:00Z" },
    });
  });

  test("a broken link names its entry and the entries that still hold before it", () => {
    // GIVEN a report broken at the entry 217 of a journal never purged
    const broken = report({
      ok: false,
      entries_checked: 218,
      first_break: { global_seq: 217, run_id: "run-9", reason: "global_hash_mismatch" },
    });

    // WHEN the verdict is read
    const verdict = verdictOf(broken);

    // THEN the alteration is at 217 and the 217 entries before it are intact
    expect(verdict).toEqual({ kind: "altered", entry: 217, intactBefore: 217, since: { from: "origin" } });
    expect(isSound(verdict)).toBe(false);
  });

  test("after a purge the intact entries are counted from the first one kept", () => {
    // GIVEN a journal that starts at 200 after a purge and breaks at 217
    const broken = report({
      ok: false,
      first_break: { global_seq: 217, run_id: "run-9", reason: "global_seq_gap" },
      start: { global_seq: 200, ts: "2026-09-12T08:00:00Z", purged_at: "2026-10-01T00:00:00Z" },
    });

    // WHEN the verdict is read
    const verdict = verdictOf(broken);

    // THEN 17 entries hold before the break, not 217
    expect(verdict).toMatchObject({ kind: "altered", entry: 217, intactBefore: 17 });
  });

  test("a break found inside one run promises nothing about the entries before it", () => {
    // GIVEN a report whose break was found while walking one run
    const broken = report({
      ok: false,
      first_break: { global_seq: 40, run_id: "run-3", reason: "per_run_broken" },
    });

    // WHEN the verdict is read
    const verdict = verdictOf(broken);

    // THEN the entry is named and no count of intact entries is claimed
    expect(verdict).toMatchObject({ kind: "altered", entry: 40, intactBefore: null });
  });

  test("an end that is not the recorded one is an alteration, with no entry to name", () => {
    // GIVEN entries that follow one another but a last one the journal did not record as its end
    const cut = report({ ok: false, entries_checked: 90, head_matches_state: false });

    // WHEN the verdict is read
    const verdict = verdictOf(cut);

    // THEN the verdict says the end was altered and is not sound
    expect(verdict).toEqual({ kind: "end_mismatch", entries: 90, since: { from: "origin" } });
    expect(isSound(verdict)).toBe(false);
  });

  test("an empty journal is not called intact", () => {
    // GIVEN a report that read no entry
    // WHEN the verdict is read
    // THEN it says there is nothing to verify
    expect(verdictOf(report({ entries_checked: 0, runs_checked: 0 }))).toEqual({ kind: "empty" });
  });

  test("every verdict about entries says a removal at the end needs an exported anchor", () => {
    // GIVEN the verdict component and the two catalogues
    const component = read("./JournalVerdict.svelte");
    const fr = JSON.parse(read("../../lib/i18n/fr.json")).journal.verify.limit as string;
    const en = JSON.parse(read("../../lib/i18n/en.json")).journal.verify.limit as string;

    // WHEN the limit of the check is looked for
    // THEN it is rendered for every verdict but the empty one, names the anchor,
    // and the French catalogue carries its own wording of it
    expect(component).toMatch(/\{#if verdict\.kind !== "empty"\}\s*<p[^>]*data-testid="journal-verdict-limit"/);
    expect(en).toContain("anchor exported");
    expect(fr.length).toBeGreaterThan(0);
    expect(fr).not.toBe(en);
  });
});

describe("the detail of an export and the organisation policy", () => {
  test("without a policy on the detail the user decides, and the summary is the default", () => {
    // GIVEN no policy, a refused policy, and an active one silent about the detail
    const rules = [
      detailRule(null),
      detailRule({ state: "none", dir: "/etc/apollia" }),
      detailRule({ state: "invalid", dir: "/etc/apollia", reason: "signature", detail: "bad" }),
      detailRule(policy(null)),
    ];

    // WHEN the rule is read
    // THEN the box is free, and the export follows it
    expect(rules).toEqual(["free", "free", "free", "free"]);
    expect(detailFor("free", false)).toBe(false);
    expect(detailFor("free", true)).toBe(true);
  });

  test("a policy that requires the detail freezes it on, one that forbids it keeps it off", () => {
    // GIVEN a policy requiring the detail, and one forbidding it
    const required = detailRule(policy("require"));
    const forbidden = detailRule(policy("forbid"));

    // WHEN the export is asked the other way round
    // THEN what the user ticked does not change what the policy settled
    expect(required).toBe("required");
    expect(detailFor(required, false)).toBe(true);
    expect(forbidden).toBe("forbidden");
    expect(detailFor(forbidden, true)).toBe(false);
  });

  test("the dialog freezes the box when required and hides it when forbidden", () => {
    // GIVEN the export dialog
    const dialog = read("./JournalExportDialog.svelte");

    // WHEN its detail region is read
    // THEN the forbidden branch holds no box, the other disables it when required,
    // and the personal data warning follows the detail actually exported
    const [forbiddenBranch, boxBranch] = dialog.split('{#if rule === "forbidden"}')[1].split("{:else}");
    expect(forbiddenBranch).not.toContain("<Checkbox");
    expect(forbiddenBranch).toContain("journal.export.detail_forbidden");
    expect(boxBranch).toContain('disabled={rule === "required"}');
    expect(dialog).toMatch(/\{#if withDetail\}[\s\S]*journal\.export\.personal_data/);
    expect(dialog).toContain("const withDetail = $derived(detailFor(rule, wantsDetail));");
  });
});

describe("what an export is asked with", () => {
  test("the export carries the filters of the screen and no page bound", () => {
    // GIVEN the screen filtered on an agent and the refused actions of today
    const filters = { ...NO_FILTER, agent: "mail sorter", result: "refused" as const, period: "today" as const };
    const now = new Date(2026, 9, 6, 14, 32);

    // WHEN the query of the export is written
    const query = exportQueryOf(filters, false, now);

    // THEN the criteria are those of the list, without limit nor offset
    expect(query.agents).toEqual(["mail sorter"]);
    expect(query.results).toEqual(["refused"]);
    expect(query.natures).toEqual(["action", "access_change", "policy"]);
    expect(query.since).toBe(new Date(2026, 9, 6).toISOString());
    expect(query).not.toHaveProperty("limit");
    expect(query).not.toHaveProperty("offset");
  });

  test("the dialog recalls each filter in force, the period always", () => {
    // GIVEN no filter but the default period, then an agent and a family
    const none = filterMentions(NO_FILTER, named);
    const some = filterMentions({ ...NO_FILTER, agent: "watch", family: "read", period: "all" }, named);

    // WHEN the mentions are listed
    // THEN the period is always there, after the criteria chosen
    expect(none).toEqual([{ key: "journal.period.last_7_days" }]);
    expect(some).toEqual([
      { text: "watch" },
      { key: "agent_sheet.family_read" },
      { key: "journal.period.all" },
    ]);
  });

  test("the file proposed is dated on the clock of this machine", () => {
    // GIVEN the sixth of October 2026
    // WHEN the name is proposed
    // THEN it is a CSV file bearing that day
    expect(exportFileName(new Date(2026, 9, 6, 9, 5))).toBe("journal-2026-10-06.csv");
  });
});

describe("the page to print", () => {
  test("a summary row says what happened in the words of the screen, without content", () => {
    // GIVEN an export of one sent message, without detail
    const document: JournalExportDocument = { detail: false, count: 1, entries: [entry()] };

    // WHEN the rows are laid out
    const [row] = printRows(document, named);

    // THEN the row carries the sentence, the labels, its place in the chain, and no detail
    expect(row.what).toEqual({
      key: "journal.sentence.tool_done_send_external",
      values: { agent: "mail sorter" },
    });
    expect(row.family).toEqual({ key: "agent_sheet.family_send_external" });
    expect(row.outcome).toEqual({ key: "journal.state.success" });
    expect([row.runId, row.seq, row.hash]).toEqual(["run-1", 7, "9f2c"]);
    expect(row.detail).toBeNull();
  });

  test("a row with its detail carries the parameters and the output of the call", () => {
    // GIVEN an export with the detail of a call
    const detailed = entry({
      detail: {
        payload: { tool_name: "mail_send" },
        tool_call: { tool_call_id: "c-1", arguments: { to: "client@example.org" }, output: null, status: "success" },
      },
    });

    // WHEN the rows are laid out
    const [row] = printRows({ detail: true, count: 1, entries: [detailed] }, named);

    // THEN the parameters are there as text, and an output the journal did not keep is absent
    expect(row.detail?.arguments).toContain("client@example.org");
    expect(row.detail?.output).toBeNull();
    expect(row.detail?.status).toBe("success");
  });

  test("an entry whose sentence needs its payload keeps the words of the journal without it", () => {
    // GIVEN an access change exported without detail, then with it
    const bare = entry({ kind: "access_changed", nature: "access_change", what: "Access revoked" });
    const full = entry({
      kind: "access_changed",
      nature: "access_change",
      what: "Access revoked",
      detail: { payload: { change: "revoked" }, tool_call: null },
    });

    // WHEN the rows are laid out
    const [without] = printRows({ detail: false, count: 1, entries: [bare] }, named);
    const [withPayload] = printRows({ detail: true, count: 1, entries: [full] }, named);

    // THEN no sentence is guessed without the payload, and the right one is used with it
    expect(without.what).toEqual({ text: "Access revoked" });
    expect(withPayload.what).toMatchObject({ key: "journal.sentence.access_revoked" });
  });

  test("on paper only the page is left, and no file is produced by the application", () => {
    // GIVEN the print stylesheet and the sheet
    const css = read("./journal-print.css");
    const sheet = read("./JournalPrintSheet.svelte");

    // WHEN their print rules are read
    // THEN every other child of the body is taken out and the page flows over several pages
    expect(css).toMatch(/@media print[\s\S]*body > \* \{\s*display: none !important;/);
    expect(css).toMatch(/body > \[data-testid="journal-print"\] > div \{[\s\S]*max-height: none !important;/);
    expect(sheet).toContain('data-testid="journal-print"');
    expect(sheet).toContain('import "./journal-print.css";');
  });
});

describe("the two actions of the journal header", () => {
  test("verifying calls the verification of the whole journal and keeps its verdict", async () => {
    // GIVEN a journal broken at the entry 217
    const verify = vi.fn(() =>
      Promise.resolve(
        report({ ok: false, first_break: { global_seq: 217, run_id: "r", reason: "global_hash_mismatch" } }),
      ),
    );
    const actions = createJournalActions(ipcOf({ verify }));
    const at = new Date(2026, 9, 6, 14, 33);

    // WHEN the user verifies
    await actions.verify(at);

    // THEN the route was read once and the verdict names the entry
    expect(verify).toHaveBeenCalledTimes(1);
    expect(actions.verification).toMatchObject({
      status: "done",
      verdict: { kind: "altered", entry: 217 },
      checkedAt: at,
    });

    // WHEN the verdict is closed
    actions.dismissVerdict();

    // THEN the header is back to rest
    expect(actions.verification).toEqual({ status: "idle" });
  });

  test("a verification that cannot be made is a failure, never an intact verdict", async () => {
    // GIVEN a runtime without a journal
    const actions = createJournalActions(
      ipcOf({ verify: () => Promise.reject(new Error("audit journal not available")) }),
    );

    // WHEN the user verifies
    await actions.verify();

    // THEN the failure is kept with its reason
    expect(actions.verification).toEqual({ status: "failed", message: "audit journal not available" });
  });

  test("opening the export reads what the policy settles about the detail", async () => {
    // GIVEN a policy that forbids the detail
    const actions = createJournalActions(ipcOf({ policy: () => Promise.resolve(policy("forbid")) }));

    // WHEN the dialog opens
    await actions.openExport();

    // THEN it is shown with the detail forbidden
    expect(actions.exportOpen).toBe(true);
    expect(actions.detailRule).toBe("forbidden");
  });

  test("a CSV export is written where the user chose, with the detail the policy requires", async () => {
    // GIVEN a policy that requires the detail and a user who did not tick the box
    const exportTo = vi.fn((_query: unknown, _detail: boolean, path: string) => Promise.resolve({ path, bytes: 40 }));
    const actions = createJournalActions(ipcOf({ policy: () => Promise.resolve(policy("require")), exportTo }));
    await actions.openExport();
    const query = exportQueryOf(NO_FILTER, false, new Date(2026, 9, 6));

    // WHEN the export is asked as CSV
    const outcome = await actions.runExport({
      format: "csv",
      wantsDetail: false,
      query,
      pickPath: () => Promise.resolve("/tmp/journal.csv"),
    });

    // THEN the file is written with the detail, and the dialog closes
    expect(exportTo).toHaveBeenCalledWith(query, true, "/tmp/journal.csv");
    expect(outcome).toEqual({ status: "written", path: "/tmp/journal.csv" });
    expect(actions.exportOpen).toBe(false);
  });

  test("giving up in the save dialog writes nothing and leaves the export open", async () => {
    // GIVEN the export dialog open
    const exportTo = vi.fn(() => Promise.resolve({ path: "", bytes: 0 }));
    const actions = createJournalActions(ipcOf({ policy: () => Promise.resolve(policy(null)), exportTo }));
    await actions.openExport();

    // WHEN the user closes the save dialog without a path
    const outcome = await actions.runExport({
      format: "csv",
      wantsDetail: true,
      query: {},
      pickPath: () => Promise.resolve(null),
    });

    // THEN nothing is written
    expect(outcome).toEqual({ status: "cancelled" });
    expect(exportTo).not.toHaveBeenCalled();
    expect(actions.exportOpen).toBe(true);
  });

  test("a refusal of the runtime comes back with its reason", async () => {
    // GIVEN a runtime that refuses the detail though the screen read no policy
    const actions = createJournalActions(
      ipcOf({ exportTo: () => Promise.reject(new Error("the organisation policy forbids the detail")) }),
    );
    await actions.openExport();

    // WHEN the export is asked with the detail
    const outcome = await actions.runExport({
      format: "csv",
      wantsDetail: true,
      query: {},
      pickPath: () => Promise.resolve("/tmp/journal.csv"),
    });

    // THEN the failure carries the reason and the dialog stays
    expect(outcome).toEqual({ status: "failed", message: "the organisation policy forbids the detail" });
    expect(actions.exportOpen).toBe(true);
    expect(actions.exportError).toBe("the organisation policy forbids the detail");
    expect(actions.exporting).toBe(false);

    // WHEN the dialog is opened again
    await actions.openExport();

    // THEN the refusal of the previous attempt is gone
    expect(actions.exportError).toBeNull();
  });

  test("a PDF export lays the page out and asks for no file path", async () => {
    // GIVEN an export document of one entry
    const document: JournalExportDocument = { detail: false, count: 1, entries: [entry()] };
    const readExport = vi.fn(() => Promise.resolve(document));
    const pickPath = vi.fn(() => Promise.resolve("/tmp/never.csv"));
    const print = vi.fn(() => Promise.resolve());
    const actions = createJournalActions(
      ipcOf({ policy: () => Promise.resolve(policy(null)), readExport, print }),
    );
    await actions.openExport();

    // WHEN the export is asked as PDF, then printed, then the preview closed
    const outcome = await actions.runExport({ format: "pdf", wantsDetail: false, query: {}, pickPath });
    const laidOut = actions.printed;
    const refused = await actions.print();
    actions.closePrint();

    // THEN the page was laid out from the export, the system printed it, and no path was asked
    expect(outcome).toEqual({ status: "preview" });
    expect(readExport).toHaveBeenCalledWith({}, false);
    expect(laidOut).toEqual(document);
    expect(pickPath).not.toHaveBeenCalled();
    expect(print).toHaveBeenCalledTimes(1);
    expect(refused).toBeNull();
    expect(actions.printed).toBeNull();
  });
});
