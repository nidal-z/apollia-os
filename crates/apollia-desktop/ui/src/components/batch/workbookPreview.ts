/**
 * The decisions of the preview of a workbook or a CSV file in the launch
 * dialog of a batch, kept out of the components so they can be tested without
 * rendering them: what is asked of the runtime, which columns are checked,
 * what the table shows and the options a launch carries.
 */
import type { PreviewColumn, PreviewRequest, PreviewRow, SourcePreview } from "$lib/ipc/batches";
import type { SourceChoice } from "./launchBatch";

/** What the user picked on a previewed file. */
export interface PreviewSelection {
  /** The sheet picked; the proposed one when `null`. */
  sheet: string | null;
  /** The header row picked, from 1; the proposed one when `null`. */
  headerRow: number | null;
  /** The letters of the columns the agent will not receive. */
  unchecked: string[];
}

/** The selection a file starts from: what the runtime proposes, every column checked. */
export const PROPOSED_SELECTION: PreviewSelection = { sheet: null, headerRow: null, unchecked: [] };

/** What a launch carries about a previewed file. */
export interface TableChoice {
  /** The options of the source: `sheet`, `header_row`, `columns`. */
  options: Record<string, string>;
  /** How many columns the agent receives. */
  checked: number;
}

/** How a row of the preview stands against the header row. */
export type RowRole = "above" | "header" | "item";

/** The path of the file a preview is asked for, or `null` when `source` has none. */
export function previewPathOf(source: SourceChoice | null): string | null {
  if (source?.shape !== "table" || source.reader === null) return null;
  return source.reader.location;
}

/** What is asked of the runtime for the file at `path` under `selection`. */
export function previewRequest(path: string, selection: PreviewSelection): PreviewRequest {
  const request: PreviewRequest = { path };
  if (selection.sheet !== null) request.sheet = selection.sheet;
  if (selection.headerRow !== null) request.headerRow = selection.headerRow;
  return request;
}

/** The selection once another sheet is picked: its own proposed header row, every column checked. */
export function withSheet(sheet: string): PreviewSelection {
  return { sheet, headerRow: null, unchecked: [] };
}

/** The selection once another header row is picked on the shown sheet. */
export function withHeaderRow(preview: SourcePreview, selection: PreviewSelection, row: number): PreviewSelection {
  return { sheet: selection.sheet ?? preview.sheet, headerRow: row, unchecked: selection.unchecked };
}

/** The selection once the column `letter` is checked or unchecked. */
export function withColumn(selection: PreviewSelection, letter: string, on: boolean): PreviewSelection {
  const others = selection.unchecked.filter((unchecked) => unchecked !== letter);
  return { ...selection, unchecked: on ? others : [...others, letter] };
}

/** Whether the agent receives the column `letter`. */
export function isChecked(selection: PreviewSelection, letter: string): boolean {
  return !selection.unchecked.includes(letter);
}

/** The columns the agent receives, in the order of the file. */
export function checkedColumns(preview: SourcePreview, selection: PreviewSelection): PreviewColumn[] {
  return preview.columns.filter((column) => isChecked(selection, column.letter));
}

/** The position of the column `letter` in a row, column A at 0; -1 for what is not a letter. */
export function columnIndex(letter: string): number {
  const upper = letter.trim().toUpperCase();
  if (!/^[A-Z]+$/.test(upper)) return -1;
  let index = 0;
  for (const char of upper) index = index * 26 + (char.charCodeAt(0) - 64);
  return index - 1;
}

/** The text of `row` under the column `letter`, empty where the row stops short. */
export function cellOf(row: PreviewRow, letter: string): string {
  return row.cells[columnIndex(letter)] ?? "";
}

/** Where `row` stands: above the header row, the header row itself, or an item. */
export function rowRole(preview: SourcePreview, row: PreviewRow): RowRole {
  if (preview.headerRow === null) return "item";
  if (row.row === preview.headerRow) return "header";
  return row.row < preview.headerRow ? "above" : "item";
}

/** The rows a header row can be picked among: those shown, and the one in use. */
export function headerRowChoices(preview: SourcePreview): number[] {
  const rows = new Set(preview.rows.map((row) => row.row));
  if (preview.headerRow !== null) rows.add(preview.headerRow);
  return [...rows].sort((a, b) => a - b);
}

/** Whether the copy of results loses the formatting of the file: `.xls`, `.xlsb`, `.ods`. */
export function formattingLost(preview: SourcePreview | null): boolean {
  return preview?.formattingNotKept === true;
}

/**
 * What a launch carries for `preview` under `selection`: the sheet and the
 * header row shown, and the checked columns by their letters once one is
 * unchecked. A source without that last option reads every column.
 */
export function tableChoice(preview: SourcePreview | null, selection: PreviewSelection): TableChoice | null {
  if (preview === null) return null;
  const checked = checkedColumns(preview, selection);
  const options: Record<string, string> = {};
  if (preview.sheet !== null) options.sheet = preview.sheet;
  if (preview.headerRow !== null) options.header_row = String(preview.headerRow);
  if (checked.length < preview.columns.length) {
    options.columns = checked.map((column) => column.letter).join(",");
  }
  return { options, checked: checked.length };
}
