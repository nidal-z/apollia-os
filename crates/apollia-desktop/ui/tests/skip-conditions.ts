/**
 * Prints, before the run, the condition every skipped Playwright case names.
 *
 * The `list` reporter renders a skipped case as a bare dash and a title: it
 * drops the annotation text, so a corpus that skips honestly still reads to a
 * caller exactly like a corpus that skips for no reason. This global setup
 * puts the reasons back into the run output.
 *
 * The text is never written here. It is read out of the spec files themselves,
 * so the banner cannot outlive the skip it describes: unskip a file and its
 * line disappears from the run on the next invocation.
 */
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const TESTS_DIR = dirname(fileURLToPath(import.meta.url));
const SKIP_CALL = /test\.skip\(\s*true\s*,([\s\S]*?)\);/g;
const STRING_PART = /"((?:[^"\\]|\\.)*)"/g;

function specFiles(dir: string): string[] {
  const found: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) found.push(...specFiles(full));
    else if (entry.name.endsWith(".spec.ts")) found.push(full);
  }
  return found.sort((a, b) => a.localeCompare(b));
}

/** The reason strings of every `test.skip(true, "...")` in one spec file. */
function conditions(text: string): string[] {
  const reasons: string[] = [];
  for (const call of text.matchAll(SKIP_CALL)) {
    const parts = [...call[1].matchAll(STRING_PART)].map((m) => m[1]);
    if (parts.length > 0) reasons.push(parts.join(""));
  }
  return reasons;
}

function wrap(text: string, width: number, indent: string): string {
  const lines: string[] = [];
  let line = "";
  for (const word of text.split(/\s+/)) {
    if (line.length + word.length + 1 > width) {
      lines.push(line);
      line = word;
    } else {
      line = line ? `${line} ${word}` : word;
    }
  }
  if (line) lines.push(line);
  return lines.map((l) => indent + l).join("\n");
}

export default function globalSetup(): void {
  const entries: Array<[string, string[]]> = [];
  for (const file of specFiles(TESTS_DIR)) {
    const reasons = conditions(readFileSync(file, "utf8"));
    if (reasons.length > 0) entries.push([relative(TESTS_DIR, file), reasons]);
  }
  if (entries.length === 0) return;

  const total = entries.reduce((n, [, reasons]) => n + reasons.length, 0);
  console.log(
    `\nSKIPPED ON PURPOSE: ${total} condition(s) across ${entries.length} ` +
      `spec file(s). Each names what has to be true for the cases it covers ` +
      `to run again.\n`,
  );
  for (const [file, reasons] of entries) {
    console.log(`  tests/${file}`);
    for (const reason of reasons) console.log(wrap(reason, 74, "    "));
    console.log("");
  }
}
