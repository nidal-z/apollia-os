/**
 * Streaming content parser.
 *
 * Segments an incrementally-growing token stream into typed blocks
 * (`text`, `thinking`, `tool`) without relying on fragile regex replaces.
 * Safe to call on every buffer update - the parser performs a single
 * linear scan with no backtracking.
 */

export type StreamBlockType = "text" | "thinking" | "tool";

export interface StreamBlock {
  type: StreamBlockType;
  content: string;
  /** `false` when the block is still being streamed (open tag, no closer yet). */
  closed: boolean;
}

interface TagSpec {
  type: StreamBlockType;
  open: string;
  close: string;
}

const TAGS: readonly TagSpec[] = [
  { type: "thinking", open: "<think>", close: "</think>" },
  { type: "tool", open: "<tool>", close: "</tool>" },
];

interface NextTag {
  spec: TagSpec;
  index: number;
}

function findNextTag(src: string, from: number): NextTag | null {
  let best: NextTag | null = null;
  for (const spec of TAGS) {
    const idx = src.indexOf(spec.open, from);
    if (idx === -1) continue;
    if (best === null || idx < best.index) {
      best = { spec, index: idx };
    }
  }
  return best;
}

export function parseStream(text: string): StreamBlock[] {
  if (!text) return [];

  const result: StreamBlock[] = [];
  let cursor = 0;

  while (cursor < text.length) {
    const next = findNextTag(text, cursor);

    if (next === null) {
      const rest = text.slice(cursor);
      if (rest) result.push({ type: "text", content: rest, closed: true });
      break;
    }

    if (next.index > cursor) {
      result.push({
        type: "text",
        content: text.slice(cursor, next.index),
        closed: true,
      });
    }

    const contentStart = next.index + next.spec.open.length;
    const endIdx = text.indexOf(next.spec.close, contentStart);

    if (endIdx === -1) {
      const content = text.slice(contentStart);
      result.push({ type: next.spec.type, content, closed: false });
      cursor = text.length;
    } else {
      const content = text.slice(contentStart, endIdx);
      result.push({ type: next.spec.type, content, closed: true });
      cursor = endIdx + next.spec.close.length;
    }
  }

  return result;
}

/** True when the final block is an unclosed `thinking` block. */
export function isThinking(blocks: readonly StreamBlock[]): boolean {
  const last = blocks.at(-1);
  return last?.type === "thinking" && !last.closed;
}
