/**
 * Small formatting rules shared by the tool step bodies: which icon a file
 * takes, how a size, a day or a duration reads, and what an edit changed.
 */

import { FileText, Sheet } from "lucide-svelte";
import type { ToolDisplayInfo } from "$lib/tools/tool-display";
import { formatSecondsLocalized } from "./duration";

const SHEET_EXTENSIONS = new Set(["xlsx", "xls", "xlsm", "ods", "csv", "tsv"]);

/** The icon of a file by its extension: a sheet for a spreadsheet, else a page. */
export function fileIcon(name: string): ToolDisplayInfo["icon"] {
  const dot = name.lastIndexOf(".");
  const ext = dot >= 0 ? name.slice(dot + 1).toLowerCase() : "";
  return SHEET_EXTENSIONS.has(ext) ? Sheet : FileText;
}

/** A size value with its unit, the value grouped and rounded for the locale. */
export function formatSize(value: number, unit: string, loc: string): string {
  let n: string;
  try {
    n = new Intl.NumberFormat(loc, { maximumFractionDigits: 1 }).format(value);
  } catch {
    n = String(value);
  }
  return `${n} ${unit}`;
}

/** A day and month for the locale ("14 février", "February 14"), or "". */
export function formatDay(iso: string | null, loc: string): string {
  if (!iso) return "";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  try {
    return new Intl.DateTimeFormat(loc, { day: "numeric", month: "long" }).format(date);
  } catch {
    return "";
  }
}

/** A step duration: milliseconds under a second, else seconds with one digit. */
export function formatStepDuration(ms: number | null | undefined, loc: string): string {
  if (ms == null || ms <= 0) return "";
  if (ms < 1000) return `${Math.round(ms)} ms`;
  return `${formatSecondsLocalized(ms, loc)} s`;
}

/** One line of an edit, marked as removed or added. */
export interface DiffLine {
  sign: "-" | "+";
  text: string;
}

/**
 * What an edit replaced, as removed lines then added lines. Lines both texts
 * share at the same place are left out: they did not change.
 */
export function editLines(oldText: string, newText: string): DiffLine[] {
  const before = oldText ? oldText.replace(/\n$/, "").split("\n") : [];
  const after = newText ? newText.replace(/\n$/, "").split("\n") : [];
  const removed: DiffLine[] = [];
  const added: DiffLine[] = [];
  const longest = Math.max(before.length, after.length);
  for (let i = 0; i < longest; i += 1) {
    if (before[i] === after[i]) continue;
    if (before[i] !== undefined) removed.push({ sign: "-", text: before[i] });
    if (after[i] !== undefined) added.push({ sign: "+", text: after[i] });
  }
  return [...removed, ...added];
}
