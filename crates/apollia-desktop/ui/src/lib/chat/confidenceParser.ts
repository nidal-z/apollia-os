/**
 * Confidence + citation markup parser (US-SP42-046 — Pattern P10, always-on).
 *
 * Mirrors `apollia-runtime::analyzers::confidence_parser`. Pure, zero deps.
 * Handles: `[conf:high|medium|low]…[/conf]` spans, nested, with inline
 * `[cite:<id>]` (or `[cite:a,b]`) pointers. Malformed markers are left
 * verbatim so the renderer simply falls back to classic prose.
 */

export type ConfidenceLevel = "high" | "medium" | "low";

export type SourceType = "web" | "document" | "tool" | "memory" | "other";

export interface Citation {
  id: string;
  title: string;
  url?: string;
  excerpt?: string;
  source_type?: SourceType;
}

export interface AssertionRange {
  start: number;
  end: number;
}

export interface Assertion {
  text_range: AssertionRange;
  confidence: ConfidenceLevel;
  citation_ids: string[];
}

export interface ParsedMessage {
  text: string;
  assertions: Assertion[];
}

interface OpenSpan {
  level: ConfidenceLevel;
  start: number;
  citationIds: string[];
}

type Token =
  | { kind: "open"; level: ConfidenceLevel }
  | { kind: "close" }
  | { kind: "cite"; ids: string[] };

const LEVELS: Record<string, ConfidenceLevel> = {
  high: "high",
  medium: "medium",
  med: "medium",
  low: "low",
};

function readToken(input: string, from: number): { token: Token; consumed: number } | null {
  const end = input.indexOf("]", from + 1);
  if (end === -1) return null;
  const inner = input.slice(from + 1, end);
  const consumed = end - from + 1;

  if (inner.startsWith("conf:")) {
    const raw = inner.slice(5).trim().toLowerCase();
    const level = LEVELS[raw];
    if (!level) return null;
    return { token: { kind: "open", level }, consumed };
  }
  if (inner === "/conf") {
    return { token: { kind: "close" }, consumed };
  }
  if (inner.startsWith("cite:")) {
    const ids = inner
      .slice(5)
      .split(",")
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    if (ids.length === 0) return null;
    return { token: { kind: "cite", ids }, consumed };
  }
  return null;
}

/**
 * Parse confidence/citation markers out of agent-produced text.
 *
 * Returns the cleaned text (markers stripped) plus the list of assertions
 * referencing character ranges in the cleaned text.
 */
export function parseConfidence(raw: string): ParsedMessage {
  let out = "";
  const assertions: Assertion[] = [];
  const stack: OpenSpan[] = [];
  let i = 0;

  while (i < raw.length) {
    const ch = raw[i];
    if (ch === "[") {
      const parsed = readToken(raw, i);
      if (parsed) {
        const { token, consumed } = parsed;
        if (token.kind === "open") {
          stack.push({ level: token.level, start: out.length, citationIds: [] });
          i += consumed;
          continue;
        }
        if (token.kind === "close") {
          const open = stack.pop();
          if (open) {
            const endIdx = out.length;
            if (endIdx > open.start) {
              assertions.push({
                text_range: { start: open.start, end: endIdx },
                confidence: open.level,
                citation_ids: open.citationIds,
              });
            }
            i += consumed;
            continue;
          }
          // Orphan close → fall through as literal
        }
        if (token.kind === "cite") {
          const top = stack[stack.length - 1];
          if (top) {
            for (const id of token.ids) {
              if (!top.citationIds.includes(id)) {
                top.citationIds.push(id);
              }
            }
            i += consumed;
            continue;
          }
          // Orphan cite → literal
        }
      }
    }
    out += ch;
    i += 1;
  }

  return { text: out, assertions };
}

/**
 * Split the cleaned text into a sequence of segments, each either
 * unannotated prose or a span belonging to exactly one assertion. Nested
 * assertions project onto the innermost span (renderer uses the top of the
 * nesting stack to pick the badge colour).
 */
export interface Segment {
  text: string;
  assertion: Assertion | null;
}

export function segmentize(parsed: ParsedMessage): Segment[] {
  if (parsed.assertions.length === 0) {
    return [{ text: parsed.text, assertion: null }];
  }
  // Collect boundary points and classify each character by the innermost
  // enclosing assertion. Innermost = the one with smallest span covering i.
  const n = parsed.text.length;
  const owner: (Assertion | null)[] = new Array(n).fill(null);
  // Sort assertions outer-first (larger span first); inner overwrites.
  const sorted = [...parsed.assertions].sort(
    (a, b) =>
      b.text_range.end - b.text_range.start - (a.text_range.end - a.text_range.start),
  );
  for (const a of sorted) {
    for (let k = a.text_range.start; k < a.text_range.end; k++) {
      owner[k] = a;
    }
  }
  const segs: Segment[] = [];
  let cursor = 0;
  while (cursor < n) {
    const current = owner[cursor];
    let j = cursor + 1;
    while (j < n && owner[j] === current) j++;
    segs.push({ text: parsed.text.slice(cursor, j), assertion: current });
    cursor = j;
  }
  return segs;
}

/** Resolve citation ids against a citation list, dropping unknown ids. */
export function resolveCitations(
  ids: string[],
  citations: Citation[],
): Citation[] {
  const byId = new Map(citations.map((c) => [c.id, c] as const));
  const out: Citation[] = [];
  for (const id of ids) {
    const c = byId.get(id);
    if (c) out.push(c);
  }
  return out;
}
