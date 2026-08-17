/**
 * Duplicate-key detection for the two translation catalogues.
 *
 * JSON allows a repeated key, and every consumer of the catalogues keeps the
 * last occurrence and drops the earlier one without a word: `JSON.parse`, the
 * `import` Vite resolves, and `svelte-i18n` alike. A second `"connections"`
 * key appended to `fr.json` therefore removes every translation the first
 * occurrence carried, and the parsed object stays valid.
 *
 * A parsed object cannot answer the question, since the evidence is gone by
 * then. This scanner reads the raw text instead, and reports every key an
 * object declares more than once, with the lines that declare it.
 */

/** One key declared more than once inside the same object. */
export interface DuplicateKey {
  /** Dotted path of the key, from the root of the document. */
  readonly path: string;
  /** Number of times the enclosing object declares it. */
  readonly count: number;
  /** 1-based line of each declaration, in source order. */
  readonly lines: readonly number[];
}

/** The source is not valid JSON. Carries the position that refused to parse. */
export class CatalogueSyntaxError extends Error {
  readonly line: number;
  readonly column: number;

  constructor(message: string, line: number, column: number) {
    super(`${message} at line ${line}, column ${column}`);
    this.name = "CatalogueSyntaxError";
    this.line = line;
    this.column = column;
  }
}

const WHITESPACE = new Set([" ", "\t", "\n", "\r"]);

class Scanner {
  private readonly text: string;
  private index = 0;
  private readonly lineStarts: number[];
  private readonly duplicates: DuplicateKey[] = [];

  constructor(text: string) {
    this.text = text;
    this.lineStarts = [0];
    for (let i = 0; i < text.length; i += 1) {
      if (text[i] === "\n") this.lineStarts.push(i + 1);
    }
  }

  scan(): DuplicateKey[] {
    this.skipWhitespace();
    this.readValue("");
    this.skipWhitespace();
    if (this.index < this.text.length) {
      this.fail("unexpected trailing content");
    }
    return this.duplicates;
  }

  private lineOf(index: number): number {
    let low = 0;
    let high = this.lineStarts.length - 1;
    while (low < high) {
      const middle = Math.ceil((low + high) / 2);
      if (this.lineStarts[middle] <= index) low = middle;
      else high = middle - 1;
    }
    return low + 1;
  }

  private columnOf(index: number): number {
    return index - this.lineStarts[this.lineOf(index) - 1] + 1;
  }

  private fail(message: string): never {
    const at = Math.min(this.index, Math.max(this.text.length - 1, 0));
    throw new CatalogueSyntaxError(message, this.lineOf(at), this.columnOf(at));
  }

  private skipWhitespace(): void {
    while (this.index < this.text.length && WHITESPACE.has(this.text[this.index])) {
      this.index += 1;
    }
  }

  private expect(character: string): void {
    if (this.text[this.index] !== character) {
      this.fail(`expected ${character}`);
    }
    this.index += 1;
  }

  private readValue(path: string): void {
    const character = this.text[this.index];
    if (character === undefined) this.fail("unexpected end of input");
    if (character === "{") {
      this.readObject(path);
      return;
    }
    if (character === "[") {
      this.readArray(path);
      return;
    }
    if (character === '"') {
      this.readString();
      return;
    }
    this.readLiteral();
  }

  private readObject(path: string): void {
    this.expect("{");
    const seen = new Map<string, number[]>();
    this.skipWhitespace();
    if (this.text[this.index] === "}") {
      this.index += 1;
      return;
    }
    for (;;) {
      this.skipWhitespace();
      const keyIndex = this.index;
      if (this.text[this.index] !== '"') this.fail("expected a key");
      const key = this.readString();
      const lines = seen.get(key);
      if (lines === undefined) seen.set(key, [this.lineOf(keyIndex)]);
      else lines.push(this.lineOf(keyIndex));
      this.skipWhitespace();
      this.expect(":");
      this.skipWhitespace();
      this.readValue(path === "" ? key : `${path}.${key}`);
      this.skipWhitespace();
      const next = this.text[this.index];
      if (next === ",") {
        this.index += 1;
        continue;
      }
      if (next === "}") {
        this.index += 1;
        break;
      }
      this.fail("expected , or }");
    }
    for (const [key, lines] of seen) {
      if (lines.length > 1) {
        this.duplicates.push({
          path: path === "" ? key : `${path}.${key}`,
          count: lines.length,
          lines,
        });
      }
    }
  }

  private readArray(path: string): void {
    this.expect("[");
    this.skipWhitespace();
    if (this.text[this.index] === "]") {
      this.index += 1;
      return;
    }
    let position = 0;
    for (;;) {
      this.skipWhitespace();
      this.readValue(`${path}[${position}]`);
      position += 1;
      this.skipWhitespace();
      const next = this.text[this.index];
      if (next === ",") {
        this.index += 1;
        continue;
      }
      if (next === "]") {
        this.index += 1;
        return;
      }
      this.fail("expected , or ]");
    }
  }

  private readString(): string {
    this.expect('"');
    let value = "";
    for (;;) {
      const character = this.text[this.index];
      if (character === undefined) this.fail("unterminated string");
      if (character === '"') {
        this.index += 1;
        return value;
      }
      if (character === "\\") {
        const escaped = this.text[this.index + 1];
        if (escaped === undefined) this.fail("unterminated escape");
        if (escaped === "u") {
          const hex = this.text.slice(this.index + 2, this.index + 6);
          if (!/^[0-9a-fA-F]{4}$/.test(hex)) this.fail("invalid unicode escape");
          value += String.fromCharCode(Number.parseInt(hex, 16));
          this.index += 6;
          continue;
        }
        const simple: Record<string, string> = {
          '"': '"',
          "\\": "\\",
          "/": "/",
          b: "\b",
          f: "\f",
          n: "\n",
          r: "\r",
          t: "\t",
        };
        const replacement = simple[escaped];
        if (replacement === undefined) this.fail("invalid escape");
        value += replacement;
        this.index += 2;
        continue;
      }
      value += character;
      this.index += 1;
    }
  }

  private readLiteral(): void {
    const start = this.index;
    while (this.index < this.text.length && /[^\s,}\]]/.test(this.text[this.index])) {
      this.index += 1;
    }
    const literal = this.text.slice(start, this.index);
    if (literal === "") this.fail("expected a value");
    if (literal === "true" || literal === "false" || literal === "null") return;
    if (!/^-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?$/.test(literal)) {
      this.index = start;
      this.fail(`invalid value ${literal}`);
    }
  }
}

/**
 * Report every key declared more than once inside the same JSON object.
 *
 * Returns an empty array on a document where no object repeats a key, whatever
 * its nesting. Throws `CatalogueSyntaxError` when the source is not valid JSON,
 * so a malformed catalogue is a failure rather than a silent zero.
 */
export function findDuplicateKeys(source: string): DuplicateKey[] {
  return new Scanner(source).scan();
}

/** Render duplicates as one line per key, for a test or a guard to print. */
export function describeDuplicateKeys(label: string, duplicates: readonly DuplicateKey[]): string {
  if (duplicates.length === 0) return `${label}: no duplicate key`;
  const lines = duplicates.map(
    (duplicate) =>
      `  ${duplicate.path} appears ${duplicate.count} times, lines ${duplicate.lines.join(", ")}`,
  );
  return [`${label}: ${duplicates.length} duplicate key(s)`, ...lines].join("\n");
}
