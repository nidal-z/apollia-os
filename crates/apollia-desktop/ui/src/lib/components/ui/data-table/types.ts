import type { Snippet } from "svelte";

/**
 * Generic column descriptor for DataTable<T>.
 *
 * `key` identifies the column and is also used as the default sort key
 * (via `row[key]`) when `sortable` is true and no custom `cell` is provided.
 */
export interface Column<T> {
  key: keyof T & string;
  header: string;
  cell?: Snippet<[T]>;
  sortable?: boolean;
  align?: "left" | "center" | "right";
  width?: string;
  /** Tailwind breakpoint at which the column is hidden on smaller screens. */
  hideOn?: "sm" | "md" | "lg" | "xl";
}

export type SortDirection = "asc" | "desc";
