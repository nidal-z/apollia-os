/**
 * The results of a batch read as the launch asked for them: one row per
 * item, one column per field of the answer, sorted and filtered.
 *
 * A batch launched with the fields of its answer named (supplier, price,
 * compliant) is worth its table: sorted by price, filtered on "compliant:
 * no", it says in a glance what forty answers read one by one do not. A cell
 * the answer left empty, and an answer that did not give its fields, are
 * marked, so the reader sees where to read again.
 */
import type { BatchItem, BatchOutput, BatchSummary } from "$lib/ipc/batches";
import { answerFields, entryOf, statusOf, type ItemStatus } from "./batchView";

/** The fields the launch of `batch` asked of each answer, named ones only. */
export function outputsOf(batch: BatchSummary): BatchOutput[] {
  return (batch.source.task?.outputs ?? []).filter((output) => output.name.trim() !== "");
}

/** What a cell says of its value. */
export type CellMark = "missing" | null;

export interface ResultRow {
  rank: number;
  /** The item, named as the list names it. */
  title: string;
  status: ItemStatus;
  /** The value of each field, by its name; `null` where none was given. */
  cells: Record<string, string | null>;
  /** A completed answer that did not give its fields, as JSON. */
  unfitted: boolean;
}

/** The rows of `items`, each answer read into the fields `outputs` names. */
export function resultRows(items: BatchItem[], outputs: BatchOutput[]): ResultRow[] {
  return items.map((item) => {
    const status = statusOf(item);
    const given = status === "completed" ? answerFields(item.output) : null;
    const cells: Record<string, string | null> = {};
    for (const output of outputs) {
      const wanted = output.name.trim().toLowerCase();
      const found = given?.find((field) => field.name.trim().toLowerCase() === wanted);
      cells[output.name] = found === undefined || found.value.trim() === "" ? null : found.value;
    }
    return {
      rank: item.rank,
      title: entryOf(item).title,
      status,
      cells,
      unfitted: status === "completed" && given === null,
    };
  });
}

/** The mark of the cell `name` of `row`: a completed answer that left it empty. */
export function cellMark(row: ResultRow, name: string): CellMark {
  return row.status === "completed" && !row.unfitted && row.cells[name] === null ? "missing" : null;
}

/**
 * The number a cell opens with, read the way people write amounts in French
 * and in English: `3 950,00 €`, `1,234.50`, `5 semaines`. `null` when the
 * cell holds no number.
 */
export function numberOf(text: string | null): number | null {
  if (text === null) return null;
  const match = /-?\d[\d\s  .,']*/.exec(text);
  if (match === null) return null;
  let digits = match[0].replace(/[\s  ']/g, "").replace(/[.,]+$/, "");
  const comma = digits.lastIndexOf(",");
  const dot = digits.lastIndexOf(".");
  if (comma >= 0 && dot >= 0) {
    // The last separator is the decimal one; the other groups thousands.
    digits = comma > dot ? digits.replace(/\./g, "").replace(",", ".") : digits.replace(/,/g, "");
  } else if (comma >= 0) {
    const decimals = digits.length - comma - 1;
    digits = decimals === 3 && digits.indexOf(",") === comma ? digits.replace(",", "") : digits.replace(/,/g, ".");
  } else if (dot >= 0) {
    const decimals = digits.length - dot - 1;
    if (decimals === 3 && digits.indexOf(".") === dot && digits.length > 4) digits = digits.replace(".", "");
  }
  const value = Number(digits);
  return Number.isFinite(value) ? value : null;
}

export type SortDirection = "asc" | "desc";
export interface ResultSort {
  /** The field sorted on; `null` keeps the order of the source. */
  field: string | null;
  direction: SortDirection;
}

/**
 * `rows` sorted on `sort`: by number when both cells hold one, by text
 * otherwise; an empty cell always last.
 */
export function sortRows(rows: ResultRow[], sort: ResultSort, locale = "fr"): ResultRow[] {
  const field = sort.field;
  if (field === null) return [...rows].sort((a, b) => a.rank - b.rank);
  const sign = sort.direction === "asc" ? 1 : -1;
  return [...rows].sort((a, b) => {
    const left = a.cells[field] ?? null;
    const right = b.cells[field] ?? null;
    if (left === null || right === null) {
      if (left === right) return a.rank - b.rank;
      return left === null ? 1 : -1;
    }
    const x = numberOf(left);
    const y = numberOf(right);
    const order = x !== null && y !== null ? x - y : left.localeCompare(right, locale, { sensitivity: "base" });
    return order === 0 ? a.rank - b.rank : sign * order;
  });
}

/** The next sort when the header of `field` is clicked: ascending, descending, none. */
export function nextSort(sort: ResultSort, field: string): ResultSort {
  if (sort.field !== field) return { field, direction: "asc" };
  if (sort.direction === "asc") return { field, direction: "desc" };
  return { field: null, direction: "asc" };
}

/** A field whose answers fall in a few values, each with how many give it. */
export interface Facet {
  field: string;
  values: { value: string; count: number }[];
}

/** The most values a field may take to be offered as a filter. */
export const FACET_MAX_VALUES = 6;

/**
 * The fields of `rows` worth a filter: those whose completed answers take
 * between two and [`FACET_MAX_VALUES`] values, read without case, most
 * frequent first.
 */
export function facetsOf(rows: ResultRow[], outputs: BatchOutput[]): Facet[] {
  const facets: Facet[] = [];
  for (const output of outputs) {
    const counts = new Map<string, { value: string; count: number }>();
    for (const row of rows) {
      const value = row.cells[output.name];
      if (value === null || value === undefined) continue;
      const key = value.trim().toLowerCase();
      const known = counts.get(key);
      if (known) known.count += 1;
      else counts.set(key, { value: value.trim(), count: 1 });
    }
    if (counts.size < 2 || counts.size > FACET_MAX_VALUES) continue;
    facets.push({ field: output.name, values: [...counts.values()].sort((a, b) => b.count - a.count) });
  }
  return facets;
}

/** The value each filtered field must hold. */
export type ResultFilter = Record<string, string>;

/** `rows` that hold, in each filtered field, its value (without case). */
export function filterRows(rows: ResultRow[], filter: ResultFilter): ResultRow[] {
  const wanted = Object.entries(filter);
  if (wanted.length === 0) return rows;
  return rows.filter((row) =>
    wanted.every(([field, value]) => (row.cells[field] ?? "").trim().toLowerCase() === value.trim().toLowerCase()),
  );
}

/** `filter` with `field` set to `value`, or cleared when it held it already. */
export function toggleFilter(filter: ResultFilter, field: string, value: string): ResultFilter {
  const next = { ...filter };
  if ((next[field] ?? "").toLowerCase() === value.toLowerCase()) delete next[field];
  else next[field] = value;
  return next;
}

/** The rows to read again: an answer that did not give its fields, or left one empty. */
export function rowsToReview(rows: ResultRow[], outputs: BatchOutput[]): number {
  return rows.filter((row) => row.unfitted || outputs.some((output) => cellMark(row, output.name) !== null)).length;
}
