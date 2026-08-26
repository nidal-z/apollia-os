#!/usr/bin/env python3
"""Rules over Rust test bodies, held at zero or on a named two-sided ratchet.

`docs/agents/TESTING.md` and `docs/agents/FORBIDDEN.md` both state rules about
what a test body must contain, and until this file nothing read a test body.
The measure that produced it found 21 tests with no assertion of any kind, one
of them a body made only of comments, and 966 tests missing at least one of the
three GIVEN / WHEN / THEN markers the corpus declares mandatory twice.

The rules:

  no-assertion   A test that carries no way of failing is green on any tree.
                 Held at zero.
  empty-body     A body made of comments. Held at zero, and separate from the
                 rule above so the report names it rather than folding it in.
  gwt-markers    The three markers, on a per-file two-sided ratchet. Naming the
                 precondition apart from the action is what makes a test fail
                 for the reason it claims.
  home-read      A test reading the operator's real home. On a per-file
                 two-sided ratchet: reading `home_dir_or_temp()` under a
                 substituted HOME is legitimate, reading it under the real one
                 is how a suite writes into `~/.apollia`.

What this file deliberately does NOT rule on: a sleep, a port, a spawned
process, a wall-clock deadline. That subject already has a guard, the
`time-sensitive-tests` rule of `scripts/check_rust_rules.py`, which holds 165
sites on a two-sided ratchet and carries the taxonomy that says why a floor of
zero would be wrong there (for a cron source or an stdio transport, the clock
and the child process are the subject). A second guard over the same sites
would answer the same question twice and drift. The `time-sensitive` rule below
therefore checks that the other rule still exists and reports what it counts,
so removing it there is red here.

Usage:
    python3 scripts/check_rust_tests.py [rule ...] [--list] [--json OUT]
    python3 scripts/check_rust_tests.py --selftest

Exit code: 0 when every rule holds, 1 on a finding, 2 when nothing was
measured (no tracked test function found, which is a broken scan, not a
clean tree).
"""

import argparse
import json
import re
import subprocess
import sys
from collections import Counter, defaultdict
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

NOTHING_MEASURED = 2

TEST_ATTR = re.compile(
    r"#\[\s*(?:tokio::test|test|async_std::test|rstest|proptest|test_case|quickcheck)\b[^\]]*\]"
)
IGNORE_ATTR = re.compile(r"#\[\s*ignore\b([^\]]*)\]")
SERIAL_ATTR = re.compile(r"#\[\s*serial\b[^\]]*\]")
SHOULD_PANIC = re.compile(r"#\[\s*should_panic\b[^\]]*\]")
FN_DECL = re.compile(r"\b(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)")

ASSERT_RE = re.compile(
    r"\b(assert|assert_eq|assert_ne|assert_matches|debug_assert|debug_assert_eq|"
    r"prop_assert|prop_assert_eq|prop_assert_ne|assert_snapshot|assert_debug_snapshot|"
    r"assert_json_snapshot|assert_yaml_snapshot|assert_cmd_snapshot|assert_ron_snapshot|"
    r"panic|unreachable)\s*!"
)
PROPTEST_MACRO = re.compile(r"\bproptest!\s*[\{\(]")
COMMENT_RE = re.compile(r"//[^\n]*|/\*.*?\*/", re.S)
UNWRAP_RE = re.compile(r"\.(unwrap|expect|unwrap_err|expect_err)\s*\(")

# Two further ways a body states its own failure, both of which the census this
# rule was promoted from counted as "no assertion". Each is named rather than
# folded into ASSERT_RE, because each is a hole if it matches too much.
#
#   explicit-error   `return Err(...)` in a body the harness panics on. The
#                    four desktop e2e tests are written this way: their verdict
#                    travels back through `with_retry`, which panics on the
#                    second failure. A bare `?` is deliberately NOT counted:
#                    every second line of an integration test carries one.
#   static-bound     a generic function whose only job is its bound, called on
#                    a concrete type. `fn assert_send_sync<T: Send + Sync>() {}`
#                    followed by `assert_send_sync::<Handle>()` is red at
#                    compile time, which is a way to fail. The shape is
#                    required in full: a lone `fn f<T>() {}` proves nothing.
RETURN_ERR_RE = re.compile(r"\breturn\s+Err\s*\(")
STATIC_BOUND_DECL = re.compile(r"\bfn\s+(assert_[A-Za-z0-9_]*)\s*<\s*[A-Za-z0-9_]+\s*:[^>]+>")

# HOME as a test body reaches for it. The first three forms are what the census
# started with; the project's own accessors were invisible to it and hid seven
# readers, which is the reason the last group is here.
HOME_READ_RE = re.compile(
    r"dirs::home_dir|home_dir\s*\(|env::var(?:_os)?\s*\(\s*\"HOME\"|var_os\s*\(\s*\"HOME\"|"
    r"paths::(?:home_string|home_dir_or_temp|data_dir_or_err|data_dir_or_temp)\s*\(|"
    r"std::env::home_dir\s*\("
)
HOME_WRITE_RE = re.compile(
    r"set_var\s*\(\s*\"(?:HOME|APOLLIA_HOME)\"|\.env\s*\(\s*\"(?:HOME|APOLLIA_HOME)\""
)

GWT_GIVEN = re.compile(r"//.*\bGIVEN\b|/\*.*\bGIVEN\b")
GWT_WHEN = re.compile(r"//.*\bWHEN\b|/\*.*\bWHEN\b")
GWT_THEN = re.compile(r"//.*\bTHEN\b|/\*.*\bTHEN\b")


# ── the tables ───────────────────────────────────────────────────────────────
#
# Both tables are ratchets, and both are read from the file below rather than
# inlined here: 197 entries in the middle of a rule file hide the rule. The
# format is one `path count` per line, `#` starts a comment.
#
# An entry is lowered in the commit that removes the sites, like every other
# ratchet of this corpus. A count above the entry is red; a count below it is
# red too, until the number follows it down.
TABLES = Path(__file__).resolve().parent / "check_rust_tests_ratchets.txt"


def load_tables():
    """The two ratchets, as `{rule: {path: count}}`."""
    tables: dict[str, dict[str, int]] = {"gwt-markers": {}, "home-read": {}}
    current = None
    raw = TABLES.read_text(encoding="utf-8")
    for line in raw.split("\n"):
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        if line.startswith("[") and line.endswith("]"):
            current = line[1:-1]
            if current not in tables:
                raise SystemExit(f"{TABLES}: unknown section {current!r}")
            continue
        if current is None:
            raise SystemExit(f"{TABLES}: entry before any section: {line!r}")
        path, _, count = line.rpartition(" ")
        tables[current][path.strip()] = int(count)
    return tables


# ── the scan ─────────────────────────────────────────────────────────────────


def git_rs_files(root):
    out = subprocess.run(
        ["git", "-C", str(root), "ls-files", "-z", "--", "*.rs"],
        check=True,
        capture_output=True,
    ).stdout
    return [p for p in out.decode().split("\0") if p]


def strip_strings(src):
    """Blank string literal contents so braces in strings do not break matching.

    Comments are kept: this scan reads them. Offsets are preserved, so a slice
    of the result indexes the same bytes as the same slice of the source.
    """
    out = []
    i = 0
    n = len(src)
    while i < n:
        c = src[i]
        if c == "/" and i + 1 < n and src[i + 1] == "/":
            j = src.find("\n", i)
            j = n if j == -1 else j
            out.append(src[i:j])
            i = j
            continue
        if c == "/" and i + 1 < n and src[i + 1] == "*":
            j = src.find("*/", i + 2)
            j = n if j == -1 else j + 2
            out.append(src[i:j])
            i = j
            continue
        if c == "r" and i + 1 < n and (src[i + 1] == '"' or src[i + 1] == "#"):
            k = i + 1
            hashes = 0
            while k < n and src[k] == "#":
                hashes += 1
                k += 1
            if k < n and src[k] == '"':
                close = '"' + "#" * hashes
                j = src.find(close, k + 1)
                j = n if j == -1 else j + len(close)
                out.append(re.sub(r"[^\n]", " ", src[i:j]))
                i = j
                continue
        if c == '"':
            j = i + 1
            while j < n:
                if src[j] == "\\":
                    j += 2
                    continue
                if src[j] == '"':
                    j += 1
                    break
                j += 1
            body = src[i:j]
            out.append('"' + re.sub(r"[^\n]", " ", body[1:-1]) + '"')
            i = j
            continue
        if c == "'" and i + 2 < n and src[i + 1] in "{}\"" and src[i + 2] == "'":
            out.append("' '")
            i += 3
            continue
        out.append(c)
        i += 1
    return "".join(out)


def has_static_bound(body):
    """A bound-only generic function, instantiated on a concrete type."""
    for decl in STATIC_BOUND_DECL.finditer(body):
        if re.search(rf"\b{re.escape(decl.group(1))}\s*::\s*<", body):
            return True
    return False


def scan_source(rel, src):
    """Every test function of one file."""
    tests = []
    if "#[" not in src:
        return tests
    clean = strip_strings(src)
    for m in TEST_ATTR.finditer(clean):
        attr_start = m.start()
        line_start = clean.rfind("\n", 0, attr_start) + 1
        if "//" in clean[line_start:attr_start]:
            continue
        block_start = line_start
        while True:
            prev_end = block_start - 1
            if prev_end <= 0:
                break
            prev_start = clean.rfind("\n", 0, prev_end) + 1
            prev_line = clean[prev_start:prev_end].strip()
            if prev_line.startswith("#[") or prev_line.startswith("//"):
                block_start = prev_start
            else:
                break
        fnm = FN_DECL.search(clean, m.end())
        if not fnm:
            continue
        between = clean[m.end() : fnm.start()]
        if TEST_ATTR.search(between):
            continue
        if "}" in between and between.count("}") > between.count("{"):
            continue
        attrs = clean[block_start : fnm.start()]
        brace = clean.find("{", fnm.end())
        if brace == -1:
            continue
        depth = 0
        j = brace
        while j < len(clean):
            if clean[j] == "{":
                depth += 1
            elif clean[j] == "}":
                depth -= 1
                if depth == 0:
                    break
            j += 1
        body = clean[brace : j + 1]
        raw_body = src[brace : j + 1]
        scope = attrs + body
        assertion = (
            bool(ASSERT_RE.search(body))
            or bool(SHOULD_PANIC.search(attrs))
            or bool(PROPTEST_MACRO.search(body))
            or bool(RETURN_ERR_RE.search(body))
            or has_static_bound(body)
        )
        tests.append(
            {
                "file": rel,
                "line": clean.count("\n", 0, fnm.start()) + 1,
                "name": fnm.group(1),
                "gwt": [
                    bool(GWT_GIVEN.search(scope)),
                    bool(GWT_WHEN.search(scope)),
                    bool(GWT_THEN.search(scope)),
                ],
                "assertion": assertion,
                "unwrap": bool(UNWRAP_RE.search(body)),
                "empty": COMMENT_RE.sub("", body).strip("{} \n\t;") == "",
                "ignore": IGNORE_ATTR.search(src[block_start : fnm.start()]) is not None,
                "serial": bool(SERIAL_ATTR.search(attrs)),
                "home_read": sorted({x.group(0) for x in HOME_READ_RE.finditer(raw_body)}),
                "home_write": bool(HOME_WRITE_RE.search(raw_body)),
            }
        )
    return tests


def load(root=REPO_ROOT):
    tests = []
    for rel in git_rs_files(root):
        try:
            src = (Path(root) / rel).read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        tests.extend(scan_source(rel, src))
    return tests


# ── the rules ────────────────────────────────────────────────────────────────


def two_sided(found, table, label):
    hits = []
    for path, count in sorted(found.items()):
        allowed = table.get(path, 0)
        if count > allowed:
            hits.append(
                f"{path}: {count} {label} against a table entry of {allowed}. "
                f"Fix the new one, or move the debt into the table in the same "
                f"commit, knowingly"
            )
    for path, allowed in sorted(table.items()):
        if found.get(path, 0) < allowed:
            hits.append(
                f"{path}: table says {allowed} {label}, found {found.get(path, 0)}. "
                f"The debt went down: lower the table entry in this same commit"
            )
    return hits


def rule_no_assertion(tests, tables):
    """A test with no way of failing at all.

    An assertion is an assert-family macro, a `panic!`/`unreachable!` arm,
    `#[should_panic]`, a `proptest!` block, an explicit `return Err(...)`, or a
    bound-only generic instantiated on a concrete type.

    A body whose only check is an `.unwrap()` can fail, on its fixture rather
    than on its claim. That is weaker, and it is not the same defect: it is
    reported as an aside and not held at zero, because emptying it is a
    rewrite of eighteen tests and not the removal of a test that is green on
    every tree there is.
    """
    hits = [
        f"{t['file']}:{t['line']} {t['name']}: no assertion, and nothing else "
        f"that can fail"
        for t in tests
        if not t["assertion"] and not t["unwrap"]
    ]
    aside = [
        f"{t['file']}:{t['line']} {t['name']}"
        for t in tests
        if not t["assertion"] and t["unwrap"]
    ]
    return hits, {"test(s) checked only by an .unwrap() on the fixture (aside)": aside}


def rule_empty_body(tests, tables):
    """A test body made of comments. It was green from the day it was written."""
    hits = [f"{t['file']}:{t['line']} {t['name']}: body is comments only" for t in tests if t["empty"]]
    return hits, {}


def rule_gwt_markers(tests, tables):
    """The three markers, per file, descending only."""
    found = Counter(t["file"] for t in tests if not all(t["gwt"]))
    hits = two_sided(dict(found), tables["gwt-markers"], "test(s) without GIVEN/WHEN/THEN")
    detail = defaultdict(list)
    for t in tests:
        if not all(t["gwt"]):
            marks = "".join("GWT"[i] if v else "-" for i, v in enumerate(t["gwt"]))
            detail[t["file"]].append(f"{t['file']}:{t['line']} {t['name']} gwt={marks}")
    return hits, {"test(s) without the three markers (aside)": [x for f in sorted(detail) for x in detail[f]]}


def rule_home_read(tests, tables):
    """A test body that resolves the operator's home."""
    readers = [t for t in tests if t["home_read"]]
    found = Counter(t["file"] for t in readers)
    hits = two_sided(dict(found), tables["home-read"], "test(s) resolving HOME")
    aside = [
        f"{t['file']}:{t['line']} {t['name']} {','.join(t['home_read'])}"
        + (" [substitutes HOME]" if t["home_write"] else "")
        for t in readers
    ]
    return hits, {"test(s) resolving HOME (aside)": aside}


def rule_time_sensitive(tests, tables):
    """Delegation, not a second rule.

    Sleeps, ports, spawned processes and wall-clock deadlines in test bodies
    are held by `time-sensitive-tests` in `scripts/check_rust_rules.py`, on a
    per-file two-sided ratchet with the taxonomy that says which of them are
    the subject of their test and which are incidental. This rule owns nothing
    of that; it fails if that rule is gone, so the subject cannot lose its
    guard without one of the two files going red.
    """
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    try:
        import check_rust_rules
    except ImportError as exc:  # pragma: no cover - a broken tree, not a finding
        return [f"scripts/check_rust_rules.py cannot be imported: {exc}"], {}
    if "time-sensitive-tests" not in check_rust_rules.RULES:
        return [
            "scripts/check_rust_rules.py no longer defines the "
            "`time-sensitive-tests` rule. Sleeps, ports and spawned processes "
            "in test bodies are unguarded: restore it there rather than "
            "reimplementing it here"
        ], {}
    held = sum(check_rust_rules.TIME_SENSITIVE_TEST_COUNTS.values())
    files = len(check_rust_rules.TIME_SENSITIVE_TEST_COUNTS)
    return [], {
        f"site(s) held by time-sensitive-tests in check_rust_rules.py, "
        f"over {files} file(s) (aside)": [f"{held} site(s)"]
    }


RULES = {
    "no-assertion": rule_no_assertion,
    "empty-body": rule_empty_body,
    "gwt-markers": rule_gwt_markers,
    "home-read": rule_home_read,
    "time-sensitive": rule_time_sensitive,
}


# ── selftest ─────────────────────────────────────────────────────────────────

RED_SAMPLE = """
#[cfg(test)]
mod tests {
    #[test]
    fn no_assertion_at_all() {
        let backend = Thing;
        let _cloned = backend.clone();
    }

    #[test]
    fn comments_only() {
        // GIVEN a body
        // WHEN nothing happens
        // THEN nothing is checked
    }

    #[test]
    fn only_unwrap() {
        let v = parse("1").unwrap();
    }
}
"""

GREEN_SAMPLE = """
#[cfg(test)]
mod tests {
    #[test]
    fn asserts() {
        // GIVEN a value
        // WHEN parsed
        // THEN it is one
        assert_eq!(parse("1"), 1);
    }

    #[test]
    fn static_bound() {
        // GIVEN the handle
        // WHEN a second owner is asked for
        // THEN the type supplies one
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Handle>();
    }

    #[tokio::test]
    async fn explicit_error() {
        // GIVEN a live runtime
        // WHEN the endpoint answers
        // THEN the status is ok
        super::with_retry(|| async {
            let status = get("/health").await?;
            if status != "ok" {
                return Err("expected ok".into());
            }
            Ok(())
        })
        .await;
    }

    #[test]
    #[should_panic]
    fn panics() {
        // GIVEN / WHEN / THEN
        boom();
    }
}
"""

HOME_SAMPLE = """
#[cfg(test)]
mod tests {
    #[test]
    fn reads_the_project_accessor() {
        // GIVEN / WHEN / THEN
        assert!(apollia_core::paths::home_string().len() > 0);
    }
}
"""

FAILURES: list[str] = []


def case(name, condition, detail):
    if condition:
        print(f"  ok    {name}")
    else:
        print(f"  FAIL  {name}")
        FAILURES.append(f"{name}: {detail}")


def selftest():
    print("check_rust_tests: every rule on a red sample first, then on a green one")
    empty_tables = {"gwt-markers": {}, "home-read": {}}

    red = scan_source("crates/x/src/red.rs", RED_SAMPLE)
    case("the red sample yields three tests", len(red) == 3, f"{len(red)}")

    hits, aside = rule_no_assertion(red, empty_tables)
    case(
        "a body with no assertion is a finding",
        len(hits) == 2 and any("no_assertion_at_all" in h for h in hits),
        f"{hits!r}",
    )
    case(
        "a body checked only by an .unwrap() is an aside, not a finding",
        not any("only_unwrap" in h for h in hits)
        and any("only_unwrap" in x for xs in aside.values() for x in xs),
        f"{hits!r} {aside!r}. Folding it into the finding would put eighteen "
        f"tests behind an exclusion list on the day the rule lands",
    )
    hits, _ = rule_empty_body(red, empty_tables)
    case(
        "a body of comments is a finding of its own",
        len(hits) == 1 and "comments_only" in hits[0],
        f"{hits!r}",
    )
    hits, _ = rule_gwt_markers(red, empty_tables)
    case(
        "two of the three red tests are missing markers",
        len(hits) == 1 and "2 test(s) without GIVEN/WHEN/THEN" in hits[0],
        f"{hits!r}",
    )

    green = scan_source("crates/x/src/green.rs", GREEN_SAMPLE)
    case("the green sample yields four tests", len(green) == 4, f"{len(green)}")
    hits, aside = rule_no_assertion(green, empty_tables)
    case(
        "positive control: assert, a static bound, an explicit Err and "
        "should_panic all count as a way to fail",
        hits == [],
        f"{hits!r}. A detector that reported these would push every bound "
        f"check and every e2e body into an exclusion list, which is the rule "
        f"crying wolf until someone silences it",
    )
    hits, _ = rule_empty_body(green, empty_tables)
    case("positive control: no empty body in the green sample", hits == [], f"{hits!r}")
    hits, _ = rule_gwt_markers(green, empty_tables)
    case("positive control: the green sample carries its markers", hits == [], f"{hits!r}")

    # A bound-only generic that is never instantiated proves nothing, and must
    # not open the door the shape above opens.
    lone = scan_source(
        "crates/x/src/lone.rs",
        "#[cfg(test)]\nmod t {\n    #[test]\n    fn lone() {\n"
        "        // GIVEN / WHEN / THEN\n        fn assert_clone<T: Clone>() {}\n    }\n}\n",
    )
    hits, aside = rule_no_assertion(lone, empty_tables)
    case(
        "a bound-only generic nobody instantiates is still no assertion",
        len(hits) == 1,
        f"{hits!r}",
    )

    # The ratchet, both ways.
    hits = two_sided({"a.rs": 3}, {"a.rs": 2}, "thing(s)")
    case("a count above its entry is red", len(hits) == 1 and "against a table entry" in hits[0], f"{hits!r}")
    hits = two_sided({"a.rs": 1}, {"a.rs": 2}, "thing(s)")
    case("a count below its entry is red too", len(hits) == 1 and "went down" in hits[0], f"{hits!r}")
    hits = two_sided({"a.rs": 2}, {"a.rs": 2}, "thing(s)")
    case("positive control: a count on its entry is silent", hits == [], f"{hits!r}")

    # HOME, the accessors the census could not see.
    home = scan_source("crates/x/src/home.rs", HOME_SAMPLE)
    hits, aside = rule_home_read(home, empty_tables)
    case(
        "the project's own home accessor is a HOME read",
        len(hits) == 1 and "1 test(s) resolving HOME" in hits[0],
        f"{hits!r}. `paths::home_string()` was invisible to the census this "
        f"rule was promoted from, which is how seven readers went uncounted",
    )
    hits, _ = rule_home_read(green, empty_tables)
    case("positive control: a test touching no home is not a reader", hits == [], f"{hits!r}")

    # The delegation. Removing the other rule has to be red here.
    hits, aside = rule_time_sensitive([], empty_tables)
    case(
        "the time-sensitive rule of check_rust_rules.py is present",
        hits == [] and any("held by time-sensitive-tests" in k for k in aside),
        f"{hits!r} {aside!r}",
    )
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import check_rust_rules

    saved = check_rust_rules.RULES.pop("time-sensitive-tests")
    try:
        hits, _ = rule_time_sensitive([], empty_tables)
        case(
            "its removal is red here rather than silent",
            len(hits) == 1 and "unguarded" in hits[0],
            f"{hits!r}. Without this half, the sleeps and ports would lose "
            f"their guard and both files would stay green",
        )
    finally:
        check_rust_rules.RULES["time-sensitive-tests"] = saved

    if FAILURES:
        print(f"\n{len(FAILURES)} self-test failure(s):\n", file=sys.stderr)
        for f in FAILURES:
            print(f"  {f}\n", file=sys.stderr)
        return 1
    print(
        "\nthe rules fire on a red sample and stay silent on a green one, a "
        "bound check and an explicit Err are ways to fail while a lone "
        "`.unwrap()` and an uninstantiated generic are not, both ratchets fail "
        "in both directions, the project's own home accessors are seen, and "
        "the sleep and port subject cannot lose its guard in the other file "
        "without this one going red"
    )
    return 0


# ── entry point ──────────────────────────────────────────────────────────────


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "names", nargs="*", metavar="rule", help="rule name(s) to run (default: every rule)"
    )
    parser.add_argument("--list", action="store_true", help="print every hit instead of the first eight")
    parser.add_argument("--json", metavar="OUT", help="write the scan to a JSON file")
    parser.add_argument(
        "--selftest", action="store_true", help="replay the fixture controls instead of measuring the tree"
    )
    parser.add_argument("--root", default=str(REPO_ROOT), help="tree to measure (default: this repository)")
    args = parser.parse_args(argv[1:])

    if args.selftest:
        return selftest()
    if args.names and any(n not in RULES for n in args.names):
        print(f"unknown rule; known rules: {', '.join(RULES)}", file=sys.stderr)
        return NOTHING_MEASURED

    tests = load(args.root)
    if not tests:
        print(
            f"nothing measured: no test function found under {args.root}",
            file=sys.stderr,
        )
        return NOTHING_MEASURED
    tables = load_tables()
    files = len({t["file"] for t in tests})
    print(f"test functions scanned: {len(tests)} in {files} file(s)")

    if args.json:
        Path(args.json).write_text(json.dumps({"tests": tests}, indent=1), encoding="utf-8")

    worst = 0
    for name in args.names or list(RULES):
        hits, asides = RULES[name](tests, tables)
        print(f"\n== {name}: {len(hits)} finding(s)")
        for h in hits if args.list else hits[:8]:
            print(f"  {h}")
        if not args.list and len(hits) > 8:
            print(f"  ... {len(hits) - 8} more (--list)")
        for label, items in asides.items():
            print(f"  -- {label}: {len(items)}")
            if args.list:
                for h in items:
                    print(f"     {h}")
        worst = max(worst, 1 if hits else 0)
    if worst == 0:
        print("\nevery rule over a Rust test body holds")
    return worst


if __name__ == "__main__":
    sys.exit(main(sys.argv))
