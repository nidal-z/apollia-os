/**
 * The results of a batch as a table: the rows read from the answers, the
 * amounts read as numbers, the sort, the filters and the cells to read again.
 */
import { describe, expect, it } from "vitest";
import type { BatchItem, BatchOutput } from "$lib/ipc/batches";
import {
  cellMark,
  facetsOf,
  filterRows,
  nextSort,
  numberOf,
  resultRows,
  rowsToReview,
  sortRows,
  toggleFilter,
} from "./batchResults";

const OUTPUTS: BatchOutput[] = [
  { name: "Supplier", description: "" },
  { name: "Price", description: "" },
  { name: "Compliant", description: "" },
];

function item(rank: number, status: string, output: string | null): BatchItem {
  return {
    rank,
    origin: `quote-${rank + 1}.pdf`,
    input: { File: `/data/Quotes/quote-${rank + 1}.pdf` },
    status,
    output,
    sources: null,
    taskId: null,
    updatedAt: "2026-10-19T08:00:00Z",
  };
}

const ITEMS = [
  item(0, "completed", '{"Supplier": "Bellier", "Price": "3 950,00 €", "Compliant": "oui"}'),
  item(1, "completed", '```json\n{"supplier": "Lambert", "price": "3 720 €", "compliant": "non"}\n```'),
  item(2, "completed", '{"Supplier": "Fauquet", "Price": "", "Compliant": "Non"}'),
  item(3, "completed", "Fauquet gives a price with tax only."),
  item(4, "failed", "the step timed out"),
];

describe("the rows of the results", () => {
  it("reads each answer into the fields asked, whatever their case, and marks what is missing", () => {
    // GIVEN answers in fields, fenced or not, one with a field left empty, one
    // in plain words, and a failure
    // WHEN the rows are read
    const rows = resultRows(ITEMS, OUTPUTS);

    // THEN each field is a cell, named as the launch named it
    expect(rows[1].cells).toEqual({ Supplier: "Lambert", Price: "3 720 €", Compliant: "non" });
    // AND the empty field is marked, the answer in plain words is unfitted,
    // and a failure marks nothing
    expect(cellMark(rows[2], "Price")).toBe("missing");
    expect(rows[3].unfitted).toBe(true);
    expect(cellMark(rows[4], "Price")).toBeNull();
    expect(rowsToReview(rows, OUTPUTS)).toBe(2);
    // AND a file of a folder is named by its path in the folder
    expect(rows[0].title).toBe("quote-1.pdf");
  });
});

describe("the amounts of the results", () => {
  it("reads amounts written the French and the English way", () => {
    // GIVEN amounts, a lead time and text
    // WHEN each is read as a number
    // THEN the separators are understood
    expect(numberOf("3 950,00 €")).toBe(3950);
    expect(numberOf("1,234.50")).toBe(1234.5);
    expect(numberOf("1.234,50 €")).toBe(1234.5);
    expect(numberOf("5 semaines")).toBe(5);
    expect(numberOf("12.5")).toBe(12.5);
    expect(numberOf("not specified")).toBeNull();
    expect(numberOf(null)).toBeNull();
  });

  it("sorts by number, an empty cell last, and cycles the sort on a header", () => {
    // GIVEN the rows of the results
    const rows = resultRows(ITEMS, OUTPUTS);

    // WHEN they are sorted by price, ascending then descending
    const ascending = sortRows(rows, { field: "Price", direction: "asc" }).map((row) => row.rank);
    const descending = sortRows(rows, { field: "Price", direction: "desc" }).map((row) => row.rank);

    // THEN the cheapest comes first, and the rows without a price stay last
    expect(ascending.slice(0, 2)).toEqual([1, 0]);
    expect(descending.slice(0, 2)).toEqual([0, 1]);
    expect(ascending.slice(2).sort()).toEqual([2, 3, 4]);
    // AND a header clicked again reverses, then lets the order go
    expect(nextSort({ field: null, direction: "asc" }, "Price")).toEqual({ field: "Price", direction: "asc" });
    expect(nextSort({ field: "Price", direction: "asc" }, "Price")).toEqual({ field: "Price", direction: "desc" });
    expect(nextSort({ field: "Price", direction: "desc" }, "Price")).toEqual({ field: null, direction: "asc" });
  });
});

describe("the filters of the results", () => {
  it("offers the fields of few values, counted without case, and filters on one", () => {
    // GIVEN the rows of the results
    const rows = resultRows(ITEMS, OUTPUTS);

    // WHEN the filters are offered
    const facets = facetsOf(rows, OUTPUTS);

    // THEN "Compliant" is offered with its two values counted, the other
    // fields have one value each or too few answers to be offered
    expect(facets.find((facet) => facet.field === "Compliant")?.values).toEqual([
      { value: "non", count: 2 },
      { value: "oui", count: 1 },
    ]);
    // AND filtering on "non" keeps those two rows, and the same value again clears it
    const filter = toggleFilter({}, "Compliant", "non");
    expect(filterRows(rows, filter).map((row) => row.rank)).toEqual([1, 2]);
    expect(toggleFilter(filter, "Compliant", "NON")).toEqual({});
  });
});
