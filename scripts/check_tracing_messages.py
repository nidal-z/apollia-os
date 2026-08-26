#!/usr/bin/env python3
"""Fail when the static message of a tracing event is a sentence instead of a label.

`docs/agents/OBSERVABILITY.md` section 3 states the rule: the static message of
a tracing event is a label, `domain.action[.qualifier]`, lowercase and
dot-separated; the fields carry the data. `docs/agents/FORBIDDEN.md` restates
half of it, the half about format strings, and `scripts/check_rust_rules.py`
holds that half at zero. The other half, the one that decides whether a log
line can be grouped at all, had no instrument: 837 of the 995 production events
of this tree were English or French prose, and nothing said so.

What the rule buys is not style. A label is a key: `agent.started` groups, a
sentence does not, and a message that reads `failed to open the database: {e}`
answers a different string on every occurrence, so the only queryable thing
about it is its file and line. Prose in the message is also where the data goes
to hide, because the writer stops adding fields once the sentence says it.

Four kinds are read, and only the first passes:

  label          `agent.started`, `mcp.connect.timeout`
  sentence       any other literal, English or French
  format_string  a `{}` placeholder inside the message
  no_message     a call with no positional literal at all

The corpus is `git ls-files -- crates/*/src/*.rs`, not a walk of the disk: a
guard that reads what the working tree happens to hold answers a different
number in an extraction of the same commit. Test code is out, and it is
excluded the way `scripts/check_panic_free.py` excludes it, by reading what a
`#[cfg(...)]` attribute binds to rather than by matching the literal string:
this file imports those helpers instead of copying them, because the copy is
what drifts.

The ratchet. This tree does not reach zero in one change, so the debt is
carried per crate in `ALLOWED` below and the ratchet only descends: a crate
above its allowance fails, a crate *below* its allowance fails too, with the
instruction to lower the number, and a crate that reaches zero leaves the list.
`apollia-runtime`, `apollia-desktop`, `apollia-oria` and `apollia-cli` are
absent on purpose. They were emptied first, and their absence is what keeps
them empty: a crate with no entry is allowed nothing.

Exit codes: 0 every crate is at its allowance, 1 at least one is off it,
2 nothing measured (no tracked source, or no tracing call in the corpus).

Usage:
    python3 scripts/check_tracing_messages.py
    python3 scripts/check_tracing_messages.py --list      # every non-conforming site
    python3 scripts/check_tracing_messages.py --json
    python3 scripts/check_tracing_messages.py --selftest  # rules replayed on a fixture
"""

import argparse
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(Path(__file__).resolve().parent))

import check_panic_free as panicfree  # noqa: E402

MACRO = re.compile(r"(?<![A-Za-z0-9_:])(?:tracing::)?(event|info|warn|error|debug|trace)!\s*\(")
LABEL = re.compile(r"^[a-z0-9_]+(?:\.[a-z0-9_]+)+$")
PLACEHOLDER = re.compile(r"(?<!\{)\{(?!\{)")
OPEN_LITERAL = re.compile(r"\s*(?:r(#*))?\"")

KINDS = ("label", "sentence", "format_string", "no_message")

# ── The ratchet ──────────────────────────────────────────────────────────────
#
# Debt carried, crate by crate, with the count each crate is allowed today.
# The list only shrinks: an entry whose crate drops below its number fails
# until the number follows it down, and an entry whose crate reaches zero
# leaves. Measured on 2026-08-26.
ALLOWED: dict[str, int] = {
    "apollia-auth": 3,
    "apollia-core": 1,
    "apollia-notifications": 18,
    "apollia-permissions": 4,
    "apollia-runner": 10,
    "apollia-stt": 5,
    "apollia-workspace": 8,
}


def _balanced(masked: str, open_index: int) -> int:
    """Index of the `)` closing the parenthesis at `open_index`, or -1."""
    depth = 0
    for index in range(open_index, len(masked)):
        char = masked[index]
        if char in "([{":
            depth += 1
        elif char in ")]}":
            depth -= 1
            if depth == 0:
                return index
    return -1


def _top_level_spans(masked_body: str) -> list[tuple[int, int]]:
    """Spans of the comma-separated arguments of one call, at depth zero."""
    spans: list[tuple[int, int]] = []
    depth = 0
    start = 0
    for index, char in enumerate(masked_body):
        if char in "([{":
            depth += 1
        elif char in ")]}":
            depth -= 1
        elif char == "," and depth == 0:
            spans.append((start, index))
            start = index + 1
    spans.append((start, len(masked_body)))
    return [(a, b) for a, b in spans if masked_body[a:b].strip()]


def _unescape(literal: str) -> str:
    return literal.replace('\\"', '"').replace("\\n", "\n").replace("\\\\", "\\")


def message_of(body: str, masked_body: str) -> str | None:
    """The first positional string literal of a call, its content, or None.

    Read on the masked body so a `,` or a `(` written inside another literal
    cannot split the arguments, and sliced out of the real body so the message
    itself is the text the compiler sees.
    """
    for start, end in _top_level_spans(masked_body):
        piece = masked_body[start:end]
        opening = OPEN_LITERAL.match(piece)
        if not opening:
            continue
        closer = '"' + (opening.group(1) or "")
        close = piece.find(closer, opening.end())
        if close == -1:
            return None
        return _unescape(body[start + opening.end() : start + close])
    return None


def classify(message: str | None) -> str:
    if message is None:
        return "no_message"
    if PLACEHOLDER.search(message):
        return "format_string"
    if LABEL.match(message):
        return "label"
    return "sentence"


def sites(text: str) -> list[dict]:
    """Every production tracing call of one file, with its message and kind.

    Pure, so the selftest can drive it on a sample without touching the tree.
    """
    masked = panicfree.blank_comments_and_strings(text)
    characters = list(masked)
    for start, end in panicfree.test_regions(masked):
        for index in range(start, end):
            if characters[index] != "\n":
                characters[index] = " "
    masked = "".join(characters)

    found: list[dict] = []
    for match in MACRO.finditer(masked):
        open_index = match.end() - 1
        close = _balanced(masked, open_index)
        if close == -1:
            continue
        body = text[open_index + 1 : close]
        masked_body = masked[open_index + 1 : close]
        message = message_of(body, masked_body)
        found.append(
            {
                "line": masked.count("\n", 0, match.start()) + 1,
                "macro": match.group(1),
                "message": message,
                "kind": classify(message),
            }
        )
    return found


def crate_of(path: str) -> str:
    parts = Path(path).parts
    return parts[1] if len(parts) > 1 and parts[0] == "crates" else parts[0]


def measure(paths: list[str], read) -> tuple[list[dict], dict[str, int]]:
    """Every production site of the corpus, and the kind histogram."""
    excluded = panicfree.excluded_modules(paths, read)
    rows: list[dict] = []
    counts = {kind: 0 for kind in KINDS}
    for path in paths:
        if path in excluded:
            continue
        text = read(path)
        if text is None:
            continue
        for site in sites(text):
            counts[site["kind"]] += 1
            rows.append({"file": path, "crate": crate_of(path), **site})
    return rows, counts


def verdict(per_crate: dict[str, int]) -> list[str]:
    """Ratchet failures, one line each. Empty when every crate is at its number."""
    failures: list[str] = []
    for crate, count in sorted(per_crate.items()):
        allowed = ALLOWED.get(crate, 0)
        if count > allowed:
            failures.append(
                f"{crate}: {count} non-conforming message(s), {allowed} allowed. "
                f"Make the message a domain.action label and move the prose into a "
                f"field: this list only descends."
            )
        elif count < allowed:
            failures.append(
                f"{crate}: {count} non-conforming message(s) left, allowance still "
                f"{allowed}. Lower it to {count} in scripts/check_tracing_messages.py."
            )
    for crate in sorted(set(ALLOWED) - set(per_crate)):
        failures.append(
            f"{crate}: allowance of {ALLOWED[crate]} but no non-conforming message "
            f"left. Drop the entry from scripts/check_tracing_messages.py."
        )
    return failures


SAMPLE_RED = """\
pub fn open() {
    tracing::info!(path = %p, "failed to open the database");
    tracing::warn!("db.opened");
    warn!(target: "runner", line = %l);
    error!("count is {}", n);
}

#[cfg(test)]
mod tests {
    fn t() {
        tracing::info!("a sentence inside a test module");
    }
}
"""

SAMPLE_GREEN = """\
pub fn open() {
    tracing::info!(path = %p, reason = %e, "db.open.failed");
    tracing::warn!("db.opened");
    warn!(target: "runner", line = %l, "runner.stderr.line");
    error!(count = n, "batch.rejected");
}
"""


def selftest() -> int:
    failures: list[str] = []

    red = sites(SAMPLE_RED)
    kinds = [site["kind"] for site in red]
    if kinds.count("sentence") != 1:
        failures.append(f"expected one sentence outside the test module, got {kinds}")
    if kinds.count("format_string") != 1:
        failures.append(f"a format string in the message did not fire: {kinds}")
    if kinds.count("no_message") != 1:
        failures.append(f"a call with no positional literal did not fire: {kinds}")
    if kinds.count("label") != 1:
        failures.append(f"a conforming label was not recognised: {kinds}")
    if any("test module" in (site["message"] or "") for site in red):
        failures.append("a message inside a #[cfg(test)] module was counted")

    green = sites(SAMPLE_GREEN)
    off = [site for site in green if site["kind"] != "label"]
    if off:
        failures.append(f"a compliant sample still fired: {off}")
    if len(green) != 4:
        failures.append(f"the compliant sample lost sites: {len(green)} of 4 read")

    # The ratchet answers in both directions, and refuses a stale entry.
    if not verdict({"x": 3}):
        failures.append("a crate above its allowance did not fire")
    if not verdict({}) and ALLOWED:
        failures.append("a crate that disappeared from the measure did not fire")
    saved = dict(ALLOWED)
    try:
        ALLOWED.clear()
        ALLOWED["x"] = 5
        if not verdict({"x": 2}):
            failures.append("a crate below its allowance did not fire")
        if not verdict({}):
            failures.append("a stale allowance did not fire")
        if verdict({"x": 5}):
            failures.append("a crate exactly at its allowance fired")
    finally:
        ALLOWED.clear()
        ALLOWED.update(saved)

    if failures:
        for message in failures:
            print(f"  FAIL  {message}")
        print("selftest verdict: RED")
        return 1
    print(
        "  ok    sentence, format string and missing message fire; label passes; "
        "test module excluded; ratchet fires on both sides and on a stale entry"
    )
    print("selftest verdict: GREEN")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--list", action="store_true", help="print every non-conforming site")
    parser.add_argument("--json", action="store_true", help="print the counts as JSON")
    parser.add_argument("--selftest", action="store_true", help="replay the rules on a fixture")
    args = parser.parse_args(argv)

    if args.selftest:
        return selftest()

    paths = panicfree.tracked_sources()
    if not paths:
        print(
            "nothing measured: `git ls-files -- crates/*/src/*.rs` listed no file",
            file=sys.stderr,
        )
        return 2

    contents: dict[str, str | None] = {}

    def read(path: str) -> str | None:
        if path not in contents:
            target = REPO_ROOT / path
            contents[path] = (
                target.read_text(encoding="utf-8", errors="replace") if target.is_file() else None
            )
        return contents[path]

    rows, counts = measure(paths, read)
    if not rows:
        print("nothing measured: no tracing call found in the corpus", file=sys.stderr)
        return 2

    bad = [row for row in rows if row["kind"] != "label"]
    per_crate: dict[str, int] = {}
    for row in bad:
        per_crate[row["crate"]] = per_crate.get(row["crate"], 0) + 1
    failures = verdict(per_crate)

    if args.json:
        print(
            json.dumps(
                {
                    "files_scanned": len(paths),
                    "sites": len(rows),
                    "counts": counts,
                    "per_crate": dict(sorted(per_crate.items())),
                    "allowance": sum(ALLOWED.values()),
                    "failures": failures,
                },
                indent=2,
            )
        )
        return 1 if failures else 0

    if args.list:
        for row in sorted(bad, key=lambda r: (r["file"], r["line"])):
            print(f"{row['file']}:{row['line']}  {row['kind']:14}  {row['message']!r}")

    print(f"tracing messages: {len(rows)} production call(s) in {len(paths)} file(s)")
    for kind in KINDS:
        print(f"  {kind:14} {counts[kind]:5d}")
    print(f"  allowance carried: {sum(ALLOWED.values())} in {len(ALLOWED)} crate(s)")

    if failures:
        print(f"\n{len(failures)} crate(s) off their allowance:")
        for line in failures:
            print(f"  {line}")
        return 1
    print("\nevery crate is at its allowance")
    return 0


if __name__ == "__main__":
    sys.exit(main())
