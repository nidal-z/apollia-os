#!/usr/bin/env python3
"""Hold the layout of `~/.apollia` to its one source, the catalogue.

Before the catalogue existed, `paths.rs` only gave `data_dir()`: each module
composed its own file names, 105 production lines built `.apollia` by hand,
the code named 22 root databases while the seed fixture carried 17, and one
route fell back to a database in the world-writable `/tmp`. Three inventories,
three answers, no guard.

The source is `DataFile` in `crates/apollia-core/src/paths.rs`. This guard
holds three rules against it:

  1. No production string literal composes the data directory by hand:
     `.apollia` as a path segment is forbidden outside `paths.rs`, except when
     written as the user notation `~/.apollia` (help text, tilde-notation
     values the resolvers expand).
  2. No production string literal names a catalogued database file outside
     `paths.rs`: a literal equal to a catalogue name, or ending with
     `/<name>`, must go through `DataFile` instead.
  3. The catalogue and the seed fixture agree: `tests/cli/seed/schemas/`
     carries exactly one `<name>.sql` per catalogue entry.

Scope is `git ls-files -- 'crates/**/*.rs'`, production text only: comments
are blanked, test files (`tests/` directories, `tests.rs`, `*_test(s).rs`)
are skipped, and each file is cut at its `#[cfg(test)] mod`.

Exit codes:
  0  every rule holds
  1  at least one violation
  2  nothing measured (no catalogue, no inventory, or no seed)

Usage:
    python3 scripts/check_data_layout.py [--selftest]
"""

import argparse
import os
import re
import subprocess
import sys

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PATHS_RS = os.path.join("crates", "apollia-core", "src", "paths.rs")
SEED_SCHEMAS = os.path.join("tests", "cli", "seed", "schemas")

# One match arm of DataFile::file_name(): `DataFile::Chat => "chat.db",`
ARM_RE = re.compile(r'DataFile::[A-Za-z0-9_]+\s*=>\s*"([a-z0-9_\-]+\.db)"')
LEGACY_RE = re.compile(r'LEGACY_[A-Z_]*DB[A-Z_]*:\s*&str\s*=\s*"([a-z0-9_\-]+\.db)"')
# A string literal on one line of comment-blanked production text.
STRING_RE = re.compile(r'"((?:[^"\\\n]|\\.)*)"')
# `.apollia` used as a path segment: not part of a longer name such as
# `.apollia-seed-home` or `io.apollia.os`.
SEGMENT_RE = re.compile(r"\.apollia(?![A-Za-z0-9_.\-])")


def production_text(text):
    text = re.sub(r"//[^\n]*", lambda m: " " * len(m.group(0)), text)
    # Cut only an inline test module (`mod x {`): a `mod x;` declaration near
    # the top of a file must not drop the production code below it.
    m = re.search(r"#\[cfg\(test\)\]\s*(pub\s+)?mod\s+\w+\s*\{", text)
    return text[: m.start()] if m else text


def read_catalogue(root):
    """Return the set of database file names DataFile declares, plus legacy names."""
    path = os.path.join(root, PATHS_RS)
    if not os.path.exists(path):
        return None, None
    text = open(path, encoding="utf-8", errors="replace").read()
    names = set(ARM_RE.findall(text))
    legacy = set(LEGACY_RE.findall(text))
    return names, legacy


def inventory(root):
    """Production Rust files, from the git inventory so the set does not depend on the tree."""
    out = subprocess.run(
        ["git", "ls-files", "--", "crates/**/*.rs"],
        cwd=root,
        capture_output=True,
        text=True,
        check=False,
    )
    files = []
    for rel in out.stdout.splitlines():
        parts = rel.split("/")
        base = os.path.basename(rel)
        if "tests" in parts[:-1]:
            continue
        if base == "tests.rs" or base.endswith(("_test.rs", "_tests.rs")):
            continue
        if rel == PATHS_RS.replace(os.sep, "/"):
            continue
        files.append(rel)
    return files


def scan_file(rel, text, catalogue):
    """Return the rule 1 and rule 2 violations of one file's production text."""
    violations = []
    for i, line in enumerate(production_text(text).split("\n"), 1):
        for lit_m in STRING_RE.finditer(line):
            lit = lit_m.group(1)
            for seg in SEGMENT_RE.finditer(lit):
                before = lit[: seg.start()]
                if before.endswith("~/") or before.endswith("~"):
                    continue
                violations.append(
                    (rel, i, f"composes `.apollia` by hand: \"{lit[:70]}\"")
                )
            for name in catalogue:
                if lit == name or lit.endswith("/" + name) or lit.endswith("\\" + name):
                    violations.append(
                        (rel, i, f"names `{name}` outside the catalogue: \"{lit[:70]}\"")
                    )
    return violations


def seed_agreement(root, names):
    """Rule 3: one seed schema per catalogue entry, nothing more, nothing less."""
    schemas_dir = os.path.join(root, SEED_SCHEMAS)
    if not os.path.isdir(schemas_dir):
        return None
    seed = {
        f[: -len(".sql")] + ".db"
        for f in os.listdir(schemas_dir)
        if f.endswith(".sql")
    }
    problems = []
    for name in sorted(names - seed):
        problems.append(f"catalogue names {name} but {SEED_SCHEMAS}/ has no schema for it")
    for name in sorted(seed - names):
        problems.append(f"{SEED_SCHEMAS}/ carries {name[:-3]}.sql but the catalogue does not name it")
    return problems


def run(root):
    names, legacy = read_catalogue(root)
    if names is None:
        print(f"nothing measured: {PATHS_RS} not found under {root}", file=sys.stderr)
        return 2
    if not names:
        print(f"nothing measured: no DataFile arm parsed from {PATHS_RS}", file=sys.stderr)
        return 2
    files = inventory(root)
    if not files:
        print("nothing measured: git ls-files listed no production Rust file", file=sys.stderr)
        return 2

    catalogue = names | legacy
    violations = []
    for rel in files:
        path = os.path.join(root, rel)
        if not os.path.exists(path):
            continue
        text = open(path, encoding="utf-8", errors="replace").read()
        violations.extend(scan_file(rel, text, catalogue))

    seed_problems = seed_agreement(root, names)
    if seed_problems is None:
        print(f"nothing measured: {SEED_SCHEMAS}/ not found", file=sys.stderr)
        return 2

    print(
        f"catalogue: {len(names)} databases (+{len(legacy)} legacy), "
        f"{len(files)} production files scanned, seed schemas compared"
    )
    for rel, line, msg in violations:
        print(f"  {rel}:{line}: {msg}")
    for msg in seed_problems:
        print(f"  {msg}")
    bad = len(violations) + len(seed_problems)
    print(f"verdict: {'RED' if bad else 'GREEN'} ({len(violations)} literal(s), {len(seed_problems)} seed drift(s))")
    return 1 if bad else 0


# ── Selftest ─────────────────────────────────────────────────────────────────

RED_SAMPLE = """\
fn data_dir(home: &Path) -> PathBuf {
    home.join(".apollia")
}
fn db(home: &Path) -> PathBuf {
    home.join(".apollia").join("chat.db")
}
"""

CLEAN_SAMPLE = """\
fn hint() -> &'static str {
    "place the model under ~/.apollia/models/"
}
fn probe(dir: &Path) -> PathBuf {
    dir.join(".apollia-write-probe")
}
"""

TEST_ONLY_SAMPLE = """\
#[cfg(test)]
mod tests {
    fn fixture(home: &Path) -> PathBuf {
        home.join(".apollia").join("chat.db")
    }
}
"""


def selftest():
    catalogue = {"chat.db"}
    failures = []

    red = scan_file("x.rs", RED_SAMPLE, catalogue)
    if len(red) != 3:
        failures.append(f"red sample: expected 3 violations (2 segments, 1 db name), got {red}")

    clean = scan_file("x.rs", CLEAN_SAMPLE, catalogue)
    if clean:
        failures.append(f"clean sample: tilde notation and non-segment names flagged: {clean}")

    test_only = scan_file("x.rs", TEST_ONLY_SAMPLE, catalogue)
    if test_only:
        failures.append(f"test-only sample: cfg(test) module scanned: {test_only}")

    names, _legacy = read_catalogue(REPO_ROOT)
    if not names or len(names) < 20:
        failures.append(f"catalogue parse: expected 20+ entries from {PATHS_RS}, got {names}")

    if failures:
        for f in failures:
            print(f"  FAIL  {f}")
        print("selftest verdict: RED")
        return 1
    print("  ok    red sample fires, clean and test-only samples pass, catalogue parses")
    print("selftest verdict: GREEN")
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--root", default=REPO_ROOT, help="repository root (default: this checkout)")
    ap.add_argument("--selftest", action="store_true", help="drive the rules on fixtures, red first")
    ns = ap.parse_args()
    if ns.selftest:
        return selftest()
    return run(ns.root)


if __name__ == "__main__":
    sys.exit(main())
