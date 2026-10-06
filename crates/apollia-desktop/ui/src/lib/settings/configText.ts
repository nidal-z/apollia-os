// What the editor of `apollia.toml` reads in a text, without a TOML parser:
// the key each line assigns, the lines the organization policy locks, and the
// lines that differ between two texts.
//
// The validation itself is the runtime's (the configuration routes); this
// module only places marks on lines, which is why a line reading is enough.

/** One line of a line-by-line comparison. */
export interface DiffLine {
  kind: "same" | "added" | "removed";
  text: string;
  /** 1-based line in the text before, null for an added line. */
  before: number | null;
  /** 1-based line in the text after, null for a removed line. */
  after: number | null;
}

/** The lines of a text, a final newline giving a last empty line. */
export function splitLines(text: string): string[] {
  return text.split("\n");
}

/**
 * Compares two texts line by line, by their longest common subsequence: the
 * lines removed from `before`, the lines added in `after`, the rest kept.
 */
export function lineDiff(before: string, after: string): DiffLine[] {
  const a = splitLines(before);
  const b = splitLines(after);
  const n = a.length;
  const m = b.length;
  // lcs[i][j]: length of the common subsequence of a[i..] and b[j..].
  const lcs: Uint32Array[] = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1));
  for (let i = n - 1; i >= 0; i--) {
    const row = lcs[i]!;
    const next = lcs[i + 1]!;
    for (let j = m - 1; j >= 0; j--) {
      row[j] = a[i] === b[j] ? next[j + 1]! + 1 : Math.max(next[j]!, row[j + 1]!);
    }
  }
  const out: DiffLine[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      out.push({ kind: "same", text: a[i]!, before: i + 1, after: j + 1 });
      i++;
      j++;
    } else if (lcs[i + 1]![j]! >= lcs[i]![j + 1]!) {
      out.push({ kind: "removed", text: a[i]!, before: i + 1, after: null });
      i++;
    } else {
      out.push({ kind: "added", text: b[j]!, before: null, after: j + 1 });
      j++;
    }
  }
  for (; i < n; i++) out.push({ kind: "removed", text: a[i]!, before: i + 1, after: null });
  for (; j < m; j++) out.push({ kind: "added", text: b[j]!, before: null, after: j + 1 });
  return out;
}

/** True when a comparison holds at least one added or removed line. */
export function hasChanges(diff: DiffLine[]): boolean {
  return diff.some((line) => line.kind !== "same");
}

/** The 1-based lines of `after` that are not in `before`: the margin marks. */
export function changedLines(before: string, after: string): Set<number> {
  const lines = new Set<number>();
  for (const line of lineDiff(before, after)) {
    if (line.kind === "added" && line.after !== null) lines.add(line.after);
  }
  return lines;
}

function unquote(part: string): string {
  const trimmed = part.trim();
  if (
    trimmed.length >= 2 &&
    ((trimmed.startsWith('"') && trimmed.endsWith('"')) ||
      (trimmed.startsWith("'") && trimmed.endsWith("'")))
  ) {
    return trimmed.slice(1, -1);
  }
  return trimmed;
}

function dotted(raw: string): string {
  return raw
    .split(".")
    .map(unquote)
    .filter((part) => part.length > 0)
    .join(".");
}

/** Bracket balance of a value, outside its strings. */
function bracketBalance(value: string): number {
  let balance = 0;
  let quote: string | null = null;
  for (let k = 0; k < value.length; k++) {
    const c = value[k]!;
    if (quote) {
      if (c === "\\" && quote === '"') k++;
      else if (c === quote) quote = null;
      continue;
    }
    if (c === "#") break;
    if (c === '"' || c === "'") quote = c;
    else if (c === "[" || c === "{") balance++;
    else if (c === "]" || c === "}") balance--;
  }
  return balance;
}

const TABLE_HEADER = /^\[\[?\s*([^\]]+?)\s*\]\]?\s*(#.*)?$/;
const ASSIGNMENT = /^((?:"[^"]*"|'[^']*'|[A-Za-z0-9_-]+)(?:\s*\.\s*(?:"[^"]*"|'[^']*'|[A-Za-z0-9_-]+))*)\s*=(.*)$/;

/**
 * The dotted key each line assigns, by 1-based line. A value that spans lines
 * (an array, an inline table, a multi-line string) gives its key to each of
 * its lines. Comments, blank lines and table headers assign nothing.
 */
export function keyLines(text: string): Map<number, string> {
  const keys = new Map<number, string>();
  const lines = splitLines(text);
  let table = "";
  let open: { key: string; balance: number; multiline: string | null } | null = null;
  lines.forEach((raw, index) => {
    const line = index + 1;
    if (open) {
      keys.set(line, open.key);
      if (open.multiline) {
        if (raw.includes(open.multiline)) open = null;
      } else {
        open.balance += bracketBalance(raw);
        if (open.balance <= 0) open = null;
      }
      return;
    }
    const trimmed = raw.trim();
    if (trimmed.length === 0 || trimmed.startsWith("#")) return;
    const header = TABLE_HEADER.exec(trimmed);
    if (header) {
      table = dotted(header[1]!);
      return;
    }
    const assignment = ASSIGNMENT.exec(trimmed);
    if (!assignment) return;
    const key = table ? `${table}.${dotted(assignment[1]!)}` : dotted(assignment[1]!);
    keys.set(line, key);
    const value = assignment[2]!.trim();
    for (const fence of ['"""', "'''"]) {
      if (value.startsWith(fence) && !value.slice(fence.length).includes(fence)) {
        open = { key, balance: 0, multiline: fence };
        return;
      }
    }
    const balance = bracketBalance(value);
    if (balance > 0) open = { key, balance, multiline: null };
  });
  return keys;
}

/** Whether `key` is fixed by one of the `locked` dotted keys or tables. */
export function isLockedKey(key: string, locked: readonly string[]): boolean {
  return locked.some((fixed) => key === fixed || key.startsWith(`${fixed}.`));
}

/** The 1-based lines of `text` that assign a key the policy fixes. */
export function lockedLines(text: string, locked: readonly string[]): Set<number> {
  const lines = new Set<number>();
  if (locked.length === 0) return lines;
  for (const [line, key] of keyLines(text)) {
    if (isLockedKey(key, locked)) lines.add(line);
  }
  return lines;
}

function lockedContent(text: string, locked: readonly string[]): string[] {
  const lines = splitLines(text);
  return [...lockedLines(text, locked)]
    .sort((x, y) => x - y)
    .map((line) => lines[line - 1]!);
}

/**
 * Whether an edit from `before` to `after` leaves every locked line as it
 * was: none changed, removed or added. A keystroke that fails this is undone,
 * so no typing reaches a key the policy fixes.
 */
export function lockedLinesIntact(
  before: string,
  after: string,
  locked: readonly string[],
): boolean {
  if (locked.length === 0) return true;
  const was = lockedContent(before, locked);
  const now = lockedContent(after, locked);
  return was.length === now.length && was.every((line, k) => line === now[k]);
}

/** The 1-based line and column of an offset in a text. */
export function caretPosition(text: string, offset: number): { line: number; column: number } {
  const head = text.slice(0, Math.max(0, offset));
  const lastBreak = head.lastIndexOf("\n");
  return { line: head.split("\n").length, column: head.length - lastBreak };
}

/** The offset of the start of a 1-based line, clamped to the text. */
export function lineOffset(text: string, line: number): number {
  const lines = splitLines(text);
  const target = Math.min(Math.max(1, line), lines.length);
  let offset = 0;
  for (let k = 0; k < target - 1; k++) offset += lines[k]!.length + 1;
  return offset;
}
