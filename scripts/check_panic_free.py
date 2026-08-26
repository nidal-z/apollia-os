#!/usr/bin/env python3
"""Hold the production `unwrap()` count at zero, and ratchet `expect(` down.

`FORBIDDEN.md` forbids `.unwrap()` and `.expect("...")` in production Rust, and
allows a documented exemption. Two lints were supposed to carry that rule.
Neither does what the corpus believes.

  - `clippy::unwrap_used` is denied by the workspace and restated by the five
    crates that own a local lint table, yet it stayed silent on six production
    `unwrap()` in `apollia-aip`. The lint does not fire when the `Err` type is
    `Infallible`, which is exactly what `IntoPyObject for String` returns.
  - `clippy::expect_used` is denied nowhere at all. The twenty-one crate roots
    open with `#![cfg_attr(test, allow(clippy::unwrap_used,
    clippy::expect_used))]`, which hands the tests back a right no table ever
    took from them.

A lint also measures one compilation of one machine. It cannot see the code of
an absent platform, nor of a feature that is off, and it blames a macro's
`expect` on the line the macro expanded from. Measured on this tree: five
`expect` behind a feature the default build leaves off, two under
`#[cfg(windows)]` read from macOS, and one imputed to `Ok(())` in
`apollia-runner/src/main.rs`. A textual sweep sees all four, and compiles
nothing.

The rule this guard enforces, and prints on every red:

  A production `unwrap()` or `expect()` is allowed only when no other writing
  removes it. Ask, in order: does the library already offer an equivalent
  expression with no `unwrap`, at the same signature? If yes, write that
  instead. If no, can you say why it cannot fail without naming a runtime
  value? If yes, put the reason above the line as `// SAFETY: <reason>` and
  raise the ceiling in this script, in the same commit. If no, return the
  error.

Scope is `git ls-files`, content is the disk. The inventory makes the scanned
set the same in a worktree and in an extraction of the same commit, which is
what `check_no_font_cdn.py` did not do (1185 files against 1059, both green);
the disk keeps the content the contributor just wrote, before any commit.

Three exclusions, and nothing else:

  1. A file declared by `mod x;` under an outer `#[cfg(...)]` whose predicate
     names `test` or `kani`, and transitively what those files declare. An
     attribute binds to the item that immediately follows it, so contiguity is
     required: `audit_journal/mod.rs` gates `proofs`, not the `signer` and
     `subscriber` two lines below, which are production on a security path.
     `#![...]` is an inner attribute and gates nothing, and `cfg_attr` applies
     an attribute under a condition, it never removes an item: accepting either
     as a gate drops thirty-eight production files from this scan.
  2. Inside a kept file, the same attributes on a block, plus functions
     annotated `#[test]`, `#[tokio::test]` and their kin. The predicate is read,
     never the literal string `#[cfg(test)]`: seven sites of
     `apollia-llm/src/backends/anthropic.rs` live under
     `#[cfg(all(test, feature = "cloud"))]`.
  3. Comments and the inside of string literals. Doc examples such as
     `apollia-workspace/src/lib.rs:26` are prose, not production.

Known limits, written down rather than discovered later. A cfg predicate is
judged by satisfiability with `test` and `kani` off, so `cfg(not(test))` and
`cfg(any(target_os = "linux", test))` are production and only a predicate that
cannot hold outside a test build gates its item.
A `#[path = "..."]` module declaration is not resolved; this tree carries none
of those. And the guard checks that an exemption is present, never that it is
true: the ceiling is what makes a new one visible in a diff.

Exit codes: 0 clean, 1 a rule was broken, 2 nothing was measured.

Usage:
    python3 scripts/check_panic_free.py
"""

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

# The two forms the rule names, and nothing else. `panic!`, `todo!` and
# `unimplemented!` are not covered here: their production count is zero today
# and no guard holds it there.
FORMS = {"unwrap": re.compile(r"\.unwrap\(\)"), "expect": re.compile(r"\.expect\(")}

# `unwrap()` is barred outright, and every exemption has to move this number in
# the commit that adds it. It is zero because the six sites this guard was born
# with turned out to need no exemption at all: pyo3 already offered
# `IntoPyObjectExt::into_py_any`, at the same signature.
UNWRAP_EXEMPTION_CEILING = 0

# `expect(` has no lint at all, so the sweep starts from the debt instead of
# from zero. The ratchet is two-sided on purpose: a one-sided ceiling records a
# maximum for ever, and this tree has already paid for an intention nothing
# executes. Lower it in the commit that removes a site.
EXPECT_RATCHET = 51

# An exemption is a `SAFETY:` comment with something after it, within the three
# lines above the site, and it covers that one site.
EXEMPTION = re.compile(r"SAFETY:\s*\S")
EXEMPTION_WINDOW = 3

RULE = """\
A production `unwrap()` or `expect()` is allowed only when no other writing
removes it. Ask, in order: does the library already offer an equivalent
expression with no `unwrap`, at the same signature? If yes, write that instead.
If no, can you say why it cannot fail without naming a runtime value? If yes,
put the reason above the line as `// SAFETY: <reason>` and raise
UNWRAP_EXEMPTION_CEILING in this script, in the same commit. If no, return the
error."""


@dataclass(frozen=True)
class Site:
    """One `unwrap()` or `expect(` found in production code."""

    line: int
    form: str
    exempt: bool
    reason: str


# ── Reading Rust without compiling it ────────────────────────────────────────


_RAW_OPEN = re.compile(r"(?:b|c)?r(#*)\"")
_CHAR = re.compile(r"'(?:\\.[^']*|[^\\'])'")
_IDENT_CHAR = re.compile(r"[A-Za-z0-9_]")


def blank_comments_and_strings(text: str) -> str:
    """Return `text` with comments and string contents turned into spaces.

    Same length and same line breaks as the input, so every offset still maps
    to its original line. Blanking rather than deleting is what lets the brace
    matching below survive a `{` written inside a string.
    """
    out = list(text)
    i, n = 0, len(text)

    def blank(start: int, stop: int) -> None:
        for k in range(max(start, 0), min(stop, n)):
            if out[k] != "\n":
                out[k] = " "

    while i < n:
        char = text[i]
        if char == "/" and text.startswith("//", i):
            end = text.find("\n", i)
            end = n if end == -1 else end
            blank(i, end)
            i = end
        elif char == "/" and text.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if text.startswith("/*", j):
                    depth += 1
                    j += 2
                elif text.startswith("*/", j):
                    depth -= 1
                    j += 2
                else:
                    j += 1
            blank(i, j)
            i = j
        elif char in "brc" and (i == 0 or not _IDENT_CHAR.match(text[i - 1])):
            match = _RAW_OPEN.match(text, i)
            if not match:
                i += 1
                continue
            closer = '"' + match.group(1)
            end = text.find(closer, match.end())
            end = n if end == -1 else end + len(closer)
            blank(match.end(), end - len(closer))
            i = end
        elif char == '"':
            j = i + 1
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == '"':
                    j += 1
                    break
                j += 1
            blank(i + 1, j - 1)
            i = j
        elif char == "'":
            match = _CHAR.match(text, i)
            if match:
                blank(i, match.end())
                i = match.end()
            else:
                # A lifetime, `'py` and friends. Nothing to blank.
                i += 1
        else:
            i += 1
    return "".join(out)


_ATTR_START = re.compile(r"^\s*#\[")
_INNER_ATTR = re.compile(r"^\s*#!\[")
_MOD_DECL = re.compile(r"^\s*(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;")
_QUOTED = re.compile(r"\"[^\"]*\"")


def _predicate_value(predicate: str) -> str:
    """Three-valued verdict of a cfg predicate with test and kani off.

    `test` and `kani` evaluate false, every other atom is unknown, and the
    verdict is "F" only when no assignment of the unknowns can make the
    predicate hold. `any(target_os = "linux", test)` is therefore not a gate:
    it compiles on Linux with test off, and reading it as test code silently
    dropped four production items of `gpu_detection.rs` from this sweep.
    """
    tokens = re.findall(r"[A-Za-z_][A-Za-z0-9_]*|[(),]", _QUOTED.sub('""', predicate))
    position = 0

    def parse() -> str:
        nonlocal position
        name = tokens[position]
        position += 1
        if position < len(tokens) and tokens[position] == "(":
            position += 1
            arguments: list[str] = []
            while tokens[position] != ")":
                if tokens[position] == ",":
                    position += 1
                    continue
                arguments.append(parse())
            position += 1
            if name == "cfg":
                return arguments[0] if arguments else "U"
            if name == "not":
                inner = arguments[0]
                return {"T": "F", "F": "T"}.get(inner, "U")
            if name == "all":
                if "F" in arguments:
                    return "F"
                return "U" if "U" in arguments else "T"
            if name == "any":
                if "T" in arguments:
                    return "T"
                return "U" if "U" in arguments else "F"
            return "U"
        return "F" if name in ("test", "kani") else "U"

    try:
        verdict = parse()
        return verdict if position == len(tokens) else "U"
    except IndexError:
        return "U"


def gates_tests(attribute: str) -> bool:
    """True when this outer attribute puts what follows it under test or kani.

    Two shapes count: a `cfg` predicate that cannot hold outside a test or
    kani build, and a test attribute such as `#[test]` or `#[tokio::test]`.
    `cfg_attr` never counts: it applies an attribute under a condition, it
    removes no item. A predicate satisfiable with test and kani off, such as
    `cfg(any(target_os = "linux", test))` or `cfg(not(test))`, gates nothing:
    the item is production on some build this sweep must keep.
    """
    body = attribute.strip()
    if not body.startswith("#[") or body.startswith("#!["):
        return False
    body = body[2:].rstrip("]").strip()
    head = body.split("(", 1)[0].strip()
    if head.split("::")[-1] == "test":
        return True
    if head != "cfg":
        return False
    return _predicate_value(body) == "F"


def _attribute_span(lines: list[str], index: int) -> tuple[str, int]:
    """Read one outer attribute, which may run over several lines."""
    text = lines[index]
    depth = text.count("[") - text.count("]")
    last = index
    while depth > 0 and last + 1 < len(lines):
        last += 1
        text += "\n" + lines[last]
        depth += lines[last].count("[") - lines[last].count("]")
    return text, last


def _item_end(masked: str, start: int) -> int:
    """Offset just past the item beginning at `start`, block or `;`."""
    i, n = start, len(masked)
    while i < n:
        if masked[i] == ";":
            return i + 1
        if masked[i] == "{":
            depth = 0
            while i < n:
                if masked[i] == "{":
                    depth += 1
                elif masked[i] == "}":
                    depth -= 1
                    if depth == 0:
                        return i + 1
                i += 1
            return n
        i += 1
    return n


def _line_offsets(masked: str) -> list[int]:
    offsets, total = [], 0
    for line in masked.split("\n"):
        offsets.append(total)
        total += len(line) + 1
    return offsets


def test_regions(masked: str) -> list[tuple[int, int]]:
    """Offset ranges of `masked` that belong to test or kani code."""
    lines = masked.split("\n")
    offsets = _line_offsets(masked)
    regions: list[tuple[int, int]] = []
    index, gated = 0, False
    while index < len(lines):
        line = lines[index]
        stripped = line.strip()
        if not stripped:
            index += 1
            continue
        if _INNER_ATTR.match(line):
            index += 1
            continue
        if _ATTR_START.match(line):
            attribute, last = _attribute_span(lines, index)
            gated = gated or gates_tests(attribute)
            index = last + 1
            continue
        if gated:
            start = offsets[index] + (len(line) - len(line.lstrip()))
            end = _item_end(masked, start)
            regions.append((start, end))
            gated = False
            while index < len(lines) and offsets[index] < end:
                index += 1
            continue
        gated = False
        index += 1
    return regions


def gated_modules(masked: str) -> list[str]:
    """Module names this file declares under a test or kani attribute."""
    lines = masked.split("\n")
    names: list[str] = []
    index, gated = 0, False
    while index < len(lines):
        line = lines[index]
        stripped = line.strip()
        if not stripped:
            index += 1
            continue
        if _INNER_ATTR.match(line):
            index += 1
            continue
        if _ATTR_START.match(line):
            attribute, last = _attribute_span(lines, index)
            gated = gated or gates_tests(attribute)
            index = last + 1
            continue
        declaration = _MOD_DECL.match(line)
        if declaration and gated:
            names.append(declaration.group(1))
        gated = False
        index += 1
    return names


# ── The sweep itself ─────────────────────────────────────────────────────────


def sites(text: str) -> list[Site]:
    """Every production `unwrap()` or `expect(` of one file, exemptions marked.

    Pure, so `check_selftest.py` can drive it on samples without writing to the
    tree. The order of the three filters is the measurement: modules first,
    because six of them carried seventy-five of the eighty-nine sites the first
    count reported; comments before the search, because a doc example is not
    production; the exemption last, because it attaches to a site that exists.
    """
    masked = blank_comments_and_strings(text)
    characters = list(masked)
    for start, end in test_regions(masked):
        for index in range(start, end):
            if characters[index] != "\n":
                characters[index] = " "
    masked = "".join(characters)

    raw_lines = text.split("\n")
    found: list[tuple[int, str]] = []
    for form, pattern in FORMS.items():
        for match in pattern.finditer(masked):
            found.append((masked.count("\n", 0, match.start()) + 1, form))
    found.sort()

    used: set[int] = set()
    result: list[Site] = []
    for line, form in found:
        reason = ""
        for above in range(line - 1, max(line - 1 - EXEMPTION_WINDOW, 0), -1):
            if above in used:
                continue
            candidate = raw_lines[above - 1]
            if EXEMPTION.search(candidate):
                used.add(above)
                reason = candidate.strip().lstrip("/").strip()
                break
        result.append(Site(line=line, form=form, exempt=bool(reason), reason=reason))
    return result


def tracked_sources() -> list[str]:
    """Rust files under `crates/*/src/`, as the index lists them."""
    listing = subprocess.run(
        ["git", "ls-files", "--", "crates/*/src/*.rs"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if listing.returncode != 0:
        return []
    return [line for line in listing.stdout.split("\n") if line]


def _module_paths(declaring: str, name: str) -> list[str]:
    parent = Path(declaring)
    root = parent.parent if parent.name in ("mod.rs", "lib.rs", "main.rs") else parent.with_suffix("")
    return [str(root / f"{name}.rs"), str(root / name / "mod.rs")]


def excluded_modules(paths: list[str], read) -> set[str]:
    """Files declared under a test or kani attribute, transitively.

    A gated declaration excludes the file it names; that file then excludes
    everything it declares in turn, gated or not, since a module of a test
    module is test code whatever it says about itself.
    """
    known = set(paths)

    def resolve(declaring: str, names: list[str]) -> list[str]:
        out = []
        for name in names:
            out += [p for p in _module_paths(declaring, name) if p in known]
        return out

    frontier: list[str] = []
    for path in paths:
        text = read(path)
        if text is not None:
            frontier += resolve(path, gated_modules(blank_comments_and_strings(text)))

    excluded: set[str] = set()
    while frontier:
        current = frontier.pop()
        if current in excluded:
            continue
        excluded.add(current)
        text = read(current)
        if text is not None:
            frontier += resolve(current, _all_modules(text))
    return excluded


def _all_modules(text: str) -> list[str]:
    masked = blank_comments_and_strings(text)
    return [m.group(1) for line in masked.split("\n") if (m := _MOD_DECL.match(line))]


def main() -> int:
    paths = tracked_sources()
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
            contents[path] = target.read_text(encoding="utf-8", errors="replace") if target.is_file() else None
        return contents[path]

    excluded = excluded_modules(paths, read)
    production = [path for path in paths if path not in excluded]

    accused: list[str] = []
    exempted: list[str] = []
    counts = {form: 0 for form in FORMS}
    exempt_counts = {form: 0 for form in FORMS}

    for path in production:
        text = read(path)
        if text is None:
            continue
        for site in sites(text):
            if site.exempt:
                exempt_counts[site.form] += 1
                exempted.append(f"{path}:{site.line}: {site.form}, {site.reason}")
                continue
            counts[site.form] += 1
            if site.form == "unwrap":
                accused.append(f"{path}:{site.line}: .unwrap() in production, no exemption")

    print(f"production files scanned: {len(production)}")
    print(f"files excluded as test or kani modules: {len(excluded)}")
    print(
        f"unwrap: {counts['unwrap']} without exemption (ceiling 0), "
        f"{exempt_counts['unwrap']} exempted (ceiling {UNWRAP_EXEMPTION_CEILING}) | "
        f"expect: {counts['expect']} without exemption (ratchet {EXPECT_RATCHET}), "
        f"{exempt_counts['expect']} exempted"
    )
    if exempted:
        print("\nexempted sites:")
        for line in exempted:
            print(f"  {line}")

    failures: list[str] = []
    if accused:
        failures.extend(accused)
    if exempt_counts["unwrap"] > UNWRAP_EXEMPTION_CEILING:
        failures.append(
            f"{exempt_counts['unwrap']} exempted unwrap() against a ceiling of "
            f"{UNWRAP_EXEMPTION_CEILING}. Raise UNWRAP_EXEMPTION_CEILING in this "
            f"script, in the commit that adds the exemption, so a reader sees the "
            f"grant next to what it grants."
        )
    if counts["expect"] > EXPECT_RATCHET:
        failures.append(
            f"{counts['expect']} expect() without exemption against a ratchet of "
            f"{EXPECT_RATCHET}. Remove the new one, or document it as `// SAFETY:`."
        )
    elif counts["expect"] < EXPECT_RATCHET:
        failures.append(
            f"{counts['expect']} expect() without exemption against a ratchet of "
            f"{EXPECT_RATCHET}. The debt went down: lower EXPECT_RATCHET to "
            f"{counts['expect']} in this same commit, or the ratchet records a "
            f"maximum nobody will ever come back to."
        )

    if failures:
        print(f"\n{len(failures)} rule(s) broken:\n", file=sys.stderr)
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        print(f"\n{RULE}", file=sys.stderr)
        return 1

    print("\nno production unwrap(), and expect() sits on its ratchet")
    return 0


if __name__ == "__main__":
    argparse.ArgumentParser(description=__doc__.splitlines()[0]).parse_args()
    sys.exit(main())
