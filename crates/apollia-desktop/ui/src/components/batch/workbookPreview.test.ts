/**
 * The decisions of the preview of a workbook in the launch dialog of a batch:
 * what is asked of the runtime, the sheet and header row proposed and picked,
 * the columns checked, the notice of a format that loses its formatting, and
 * what the launch carries, down to the command the preview reaches.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const ipc = vi.hoisted(() => vi.fn(async (_cmd: string, _args?: unknown): Promise<unknown> => null));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc }));

import { previewBatchSource, type SourcePreview } from "$lib/ipc/batches";
import { DEFAULT_THRESHOLD, batchFileOf, buildLaunch, fileChoice, refusalOf, type LaunchForm } from "./launchBatch";
import {
  PROPOSED_SELECTION,
  cellOf,
  checkedColumns,
  columnIndex,
  formattingLost,
  headerRowChoices,
  isChecked,
  previewPathOf,
  previewRequest,
  rowRole,
  tableChoice,
  withColumn,
  withHeaderRow,
  withSheet,
} from "./workbookPreview";

function preview(over: Partial<SourcePreview> = {}): SourcePreview {
  return {
    format: "xlsx",
    sourceType: "spreadsheet",
    sheets: [
      { name: "Notes", nonEmptyRows: 2 },
      { name: "March", nonEmptyRows: 130 },
    ],
    proposedSheet: "March",
    sheet: "March",
    proposedHeaderRow: 2,
    headerRow: 2,
    columns: [
      { letter: "A", name: "Customer" },
      { letter: "B", name: "Request" },
      { letter: "D", name: "Date" },
    ],
    rows: [
      { row: 1, cells: ["Requests of March"] },
      { row: 2, cells: ["Customer", "Request", "", "Date"] },
      { row: 3, cells: ["Durand", "Refund", "", "2026-03-02"] },
      { row: 4, cells: ["Martin", "Delay"] },
    ],
    estimatedItems: 128,
    canModifyOriginal: true,
    formattingNotKept: false,
    ...over,
  };
}

function form(over: Partial<LaunchForm> = {}): LaunchForm {
  return {
    agent: "triage",
    source: fileChoice("/data/requests.xlsx"),
    granted: [],
    threshold: DEFAULT_THRESHOLD,
    destination: "copy",
    columns: "Category",
    unreadOnly: false,
    ...over,
  };
}

beforeEach(() => {
  ipc.mockReset();
  ipc.mockResolvedValue(null);
});

describe("the preview shown once a workbook or a CSV file is chosen", () => {
  it("is asked for a table file only, on the file alone at first", () => {
    // GIVEN a workbook, a CSV file, a file no batch reads, and no source
    const workbook = fileChoice("/data/requests.xlsx");
    const csv = fileChoice("/data/requests.csv");
    const pdf = fileChoice("/data/notes.pdf");

    // WHEN the path to preview is read for each
    // THEN the two tables have one, and the request names neither sheet nor header row
    expect(previewPathOf(workbook)).toBe("/data/requests.xlsx");
    expect(previewPathOf(csv)).toBe("/data/requests.csv");
    expect(previewPathOf(pdf)).toBeNull();
    expect(previewPathOf(null)).toBeNull();
    expect(previewRequest("/data/requests.xlsx", PROPOSED_SELECTION)).toEqual({ path: "/data/requests.xlsx" });
  });

  it("reaches the runtime through its command and comes back whole", async () => {
    // GIVEN a runtime that previews the file
    const answered = preview();
    ipc.mockResolvedValue(answered);

    // WHEN the dialog asks for the file under a picked sheet
    const shown = await previewBatchSource({ path: "/data/requests.xlsx", sheet: "March" });

    // THEN the command carries the file and the sheet, and leaves the header row to the runtime
    expect(ipc).toHaveBeenCalledWith("preview_batch_source", {
      path: "/data/requests.xlsx",
      sheet: "March",
      headerRow: null,
    });
    expect(shown).toEqual(answered);
  });

  it("reads an answer that is not a preview as none, and lets a refusal through", async () => {
    // GIVEN a runtime that answers nothing, then refuses
    ipc.mockResolvedValueOnce(null);
    ipc.mockRejectedValueOnce("API error (404): no file at '/data/absent.csv'");

    // WHEN the dialog asks twice
    const none = await previewBatchSource({ path: "/data/requests.csv" });
    const refused = previewBatchSource({ path: "/data/absent.csv" });

    // THEN the first is no preview, the second carries the reason
    expect(none).toBeNull();
    await expect(refused).rejects.toContain("no file at '/data/absent.csv'");
  });

  it("checks every column by default and counts the items", () => {
    // GIVEN the preview of a file as it opens
    const shown = preview();

    // WHEN the checked columns and the launch choice are read
    const checked = checkedColumns(shown, PROPOSED_SELECTION);
    const choice = tableChoice(shown, PROPOSED_SELECTION);

    // THEN every column is checked, and the launch names the sheet and header row shown
    expect(checked.map((column) => column.name)).toEqual(["Customer", "Request", "Date"]);
    expect(choice).toEqual({ options: { sheet: "March", header_row: "2" }, checked: 3 });
    expect(shown.estimatedItems).toBe(128);
  });

  it("shows each row under its columns, and tells the header row from the items", () => {
    // GIVEN a preview whose header is on row 2 and whose column C is empty
    const shown = preview();

    // WHEN the cells and the roles of the rows are read
    const roles = shown.rows.map((row) => rowRole(shown, row));
    const dates = shown.rows.map((row) => cellOf(row, "D"));

    // THEN column D is read at its own position, and a short row reads as empty
    expect(roles).toEqual(["above", "header", "item", "item"]);
    expect(dates).toEqual(["", "Date", "2026-03-02", ""]);
    expect(columnIndex("A")).toBe(0);
    expect(columnIndex("z")).toBe(25);
    expect(columnIndex("AA")).toBe(26);
    expect(columnIndex("1")).toBe(-1);
  });

  it("follows another sheet with its own proposals, and another header row on the same sheet", () => {
    // GIVEN a preview with the Date column unchecked
    const shown = preview();
    const unchecked = withColumn(PROPOSED_SELECTION, "D", false);

    // WHEN another sheet is picked, or another header row
    const sheet = withSheet("Notes");
    const header = withHeaderRow(shown, unchecked, 1);

    // THEN the sheet starts afresh, and the header row keeps the sheet and the unchecked column
    expect(previewRequest("/f.xlsx", sheet)).toEqual({ path: "/f.xlsx", sheet: "Notes" });
    expect(sheet.unchecked).toEqual([]);
    expect(previewRequest("/f.xlsx", header)).toEqual({ path: "/f.xlsx", sheet: "March", headerRow: 1 });
    expect(header.unchecked).toEqual(["D"]);
    expect(headerRowChoices(shown)).toEqual([1, 2, 3, 4]);
    expect(headerRowChoices(preview({ headerRow: 40 }))).toEqual([1, 2, 3, 4, 40]);
  });
});

describe("a format whose formatting is not kept", () => {
  it.each(["xls", "xlsb", "ods"])("says so for .%s", (format) => {
    // GIVEN the preview of a file the runtime flags as losing its formatting
    const shown = preview({ format, canModifyOriginal: false, formattingNotKept: true });

    // WHEN the notice is decided
    // THEN the preview carries it
    expect(formattingLost(shown)).toBe(true);
  });

  it("says nothing for an .xlsx workbook, a CSV file, or no preview", () => {
    // GIVEN previews the runtime does not flag
    const workbook = preview();
    const csv = preview({ format: "csv", sourceType: "csv", canModifyOriginal: false });

    // WHEN the notice is decided
    // THEN none carries it
    expect(formattingLost(workbook)).toBe(false);
    expect(formattingLost(csv)).toBe(false);
    expect(formattingLost(null)).toBe(false);
  });
});

describe("the preview opened from a file joined to the chat", () => {
  it("is asked on that file, with nothing picked yet", () => {
    // GIVEN a workbook dropped in the chat
    const dropped = { name: "requests.xlsx", absolutePath: "/Users/me/requests.xlsx" };

    // WHEN "process each row" opens the dialog on it
    const file = batchFileOf(dropped);
    const source = file === null ? null : fileChoice(file);
    const path = previewPathOf(source);

    // THEN the dialog previews that very file, as the runtime proposes it
    expect(path).toBe("/Users/me/requests.xlsx");
    expect(previewRequest(path ?? "", PROPOSED_SELECTION)).toEqual({ path: "/Users/me/requests.xlsx" });
  });
});

describe("the columns a launch sends", () => {
  it("leaves an unchecked column out of the launch", () => {
    // GIVEN a preview whose Date column is unchecked
    const shown = preview();
    const selection = withColumn(PROPOSED_SELECTION, "D", false);

    // WHEN the launch is built
    const launch = buildLaunch(form({ table: tableChoice(shown, selection) }), []);

    // THEN the source names the two columns left, by their letters
    expect(isChecked(selection, "D")).toBe(false);
    expect(launch?.source.options).toEqual({ sheet: "March", header_row: "2", columns: "A,B" });
    expect(launch?.source.write).toEqual({ destination: "copy", columns: ["Category"] });
  });

  it("checks a column again, and then names no column at all", () => {
    // GIVEN a column unchecked then checked again
    const shown = preview();
    const selection = withColumn(withColumn(PROPOSED_SELECTION, "B", false), "B", true);

    // WHEN the launch choice is read
    const choice = tableChoice(shown, selection);

    // THEN every column is sent, which the source reads without the option
    expect(choice?.options.columns).toBeUndefined();
    expect(choice?.checked).toBe(3);
  });

  it("refuses a launch with every column unchecked", () => {
    // GIVEN a preview with its three columns unchecked
    const shown = preview();
    const none = ["A", "B", "D"].reduce((selection, letter) => withColumn(selection, letter, false), PROPOSED_SELECTION);

    // WHEN the launch is decided
    const refused = form({ table: tableChoice(shown, none) });

    // THEN it is refused before it is sent
    expect(refusalOf(refused)).toBe("fields_missing");
    expect(buildLaunch(refused, [])).toBeNull();
  });

  it("sends no option while no preview is known, and none for a mailbox", () => {
    // GIVEN a file whose preview has not come back, and a mailbox
    const pending = form({ table: tableChoice(null, PROPOSED_SELECTION) });
    const mailbox = form({
      source: { key: "source:s-4", shape: "mail", label: "Support", detail: "imap:support", reader: { kind: "mail", location: "imap:support" }, blocked: null },
      table: { options: { sheet: "March" }, checked: 3 },
    });

    // WHEN both launches are built
    // THEN the runtime reads what it proposes, and a mailbox carries no sheet
    expect(buildLaunch(pending, [])?.source.options).toEqual({});
    expect(buildLaunch(mailbox, [])?.source.options).toEqual({});
  });
});
