#!/usr/bin/env python3
"""Ratchet the SQLite stores of `crates/` onto the versioned schema layer.

Every production module that creates a table must carry a schema version it
checks at open time, so a database written by a newer binary is refused
instead of misread. The layer exists (`apollia_core::schema::open_versioned`);
adoption is per store, and this guard is the ratchet: the modules that predate
the layer are named in EXEMPT_UNVERSIONED below, a module absent from that
list that creates a table without a version fails here, and a listed module
that has adopted the layer (or stopped creating tables) must leave the list in
the same commit, so the list only shrinks.

Also held at zero, with no exemption: non-idempotent DDL. A `CREATE TABLE` /
`INDEX` / `TRIGGER` without `IF NOT EXISTS`, or a bare `ALTER TABLE ... ADD
COLUMN` with no column probe or duplicate-column tolerance in its crate,
breaks the second open of the same database.

What is measured, on production Rust only (test modules and `tests/`
directories are skipped, comments are blanked, `include_str!` SQL is pulled
in): the tables each module creates, whether it carries a version mechanism
(`PRAGMA user_version`, a `SCHEMA_VERSION` constant, a `_schema_version` or
`schema_migrations` table, or `open_versioned`), and the idempotence of its
DDL.

Exit codes:
  0  every rule holds
  1  a new unversioned module, a stale exemption, or non-idempotent DDL
  2  nothing measured

Usage:
    python3 scripts/check_sqlite_schema_versioning.py [--root crates] [--selftest]
"""

import argparse
import json
import os
import re
import sys

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# The stores that predate the versioned layer, measured on 2026-08-25. Each
# entry leaves this list in the commit that migrates its module onto
# `open_versioned`, so the list only ever shrinks, toward zero. Adding an
# entry here is the one thing this guard exists to refuse: a new store starts
# versioned.
EXEMPT_UNVERSIONED = {
    "crates/apollia-desktop/src/commands/artifacts.rs",
    "crates/apollia-llm/src/repository.rs",
    "crates/apollia-memory/src/file_timestamp_cache.rs",
    "crates/apollia-oria/src/plan_cache.rs",
    "crates/apollia-oria/src/plan_repository.rs",
    "crates/apollia-triggers/src/definition_repository.rs",
}

CREATE_RE = re.compile(
    r"CREATE\s+(?:VIRTUAL\s+)?TABLE\s+(IF\s+NOT\s+EXISTS\s+)?([A-Za-z_][A-Za-z0-9_]*)", re.I
)
DDL_RE = re.compile(
    r"CREATE\s+(?:UNIQUE\s+)?(?:VIRTUAL\s+)?(TABLE|INDEX|TRIGGER)\s+(IF\s+NOT\s+EXISTS\s+)?([A-Za-z_][A-Za-z0-9_]*)",
    re.I,
)
ALTER_RE = re.compile(r"ALTER\s+TABLE\s+([A-Za-z_][A-Za-z0-9_]*)\s+ADD\s+COLUMN", re.I)
VERSION_RE = re.compile(
    r"_schema_version|schema_migrations|user_version|SCHEMA_VERSION|CURRENT_SCHEMA_VERSION|open_versioned",
    re.I,
)
PROBE_RE = re.compile(
    r"pragma_table_info|PRAGMA\s+table_info|column_exists|has_column|duplicate column|"
    r"extended_code\s*==\s*1|let _ = conn\.execute|add_column_if_missing",
    re.I,
)
INCLUDE_RE = re.compile(r'include_str!\("([^"]+\.sql)"\)')


def production_text(path, text=None):
    if text is None:
        text = open(path, encoding="utf-8", errors="replace").read()
    text = re.sub(r"//[^\n]*", lambda m: " " * len(m.group(0)), text)
    # Cut only an inline test module (`mod x {`): a `mod x;` declaration near
    # the top of a file must not drop the production code below it.
    m = re.search(r"#\[cfg\(test\)\]\s*(pub\s+)?mod\s+\w+\s*\{", text)
    text = text[: m.start()] if m else text
    for rel in INCLUDE_RE.findall(text):
        sql_path = os.path.normpath(os.path.join(os.path.dirname(path), rel))
        if os.path.exists(sql_path):
            sql = open(sql_path, encoding="utf-8", errors="replace").read()
            sql = re.sub(r"--[^\n]*", "", sql)
            text += "\n" + sql
    return text


def scan(root):
    rows = []
    for dirpath, _d, files in os.walk(root):
        if "/target" in dirpath or "/tests" in dirpath or "/node_modules" in dirpath:
            continue
        for f in sorted(files):
            if not f.endswith(".rs") or f.endswith(("_test.rs", "_tests.rs")) or f == "tests.rs":
                continue
            path = os.path.join(dirpath, f)
            text = production_text(path)
            tables = CREATE_RE.findall(text)
            if not tables:
                continue
            ddl_bad = [(k, n) for k, ifne, n in DDL_RE.findall(text) if not ifne]
            alters = ALTER_RE.findall(text)
            probe = bool(PROBE_RE.search(text))
            if not probe and alters:
                # The tolerance may live in a sibling module of the same crate
                # (audit_journal: DDL constants in actor.rs, executed in handle.rs).
                crate_dir = path.split("/src/")[0] + "/src"
                for dp, _dd, ff in os.walk(crate_dir):
                    for g in ff:
                        if g.endswith(".rs") and PROBE_RE.search(production_text(os.path.join(dp, g))):
                            probe = True
                            break
                    if probe:
                        break
            abs_path = os.path.abspath(path)
            base = REPO_ROOT if abs_path.startswith(REPO_ROOT + os.sep) else os.path.dirname(os.path.abspath(root))
            rows.append(
                {
                    "file": os.path.relpath(abs_path, base).replace(os.sep, "/"),
                    "tables": sorted({n for _i, n in tables}),
                    "versioned": bool(VERSION_RE.search(text)),
                    "ddl_not_idempotent": ddl_bad,
                    "alter_add_column": len(alters),
                    "alter_probed": probe,
                }
            )
    return rows


def judge(rows, exempt):
    unversioned = {r["file"] for r in rows if not r["versioned"]}
    nonidem = [
        r for r in rows if r["ddl_not_idempotent"] or (r["alter_add_column"] and not r["alter_probed"])
    ]
    new_unversioned = sorted(unversioned - exempt)
    stale_exempt = sorted(exempt - unversioned)
    return unversioned, nonidem, new_unversioned, stale_exempt


def run(root, as_json):
    if not os.path.isdir(root):
        print(f"root not found: {root}", file=sys.stderr)
        return 2
    rows = scan(root)
    if not rows:
        print("nothing measured: no CREATE TABLE in production code", file=sys.stderr)
        return 2
    unversioned, nonidem, new_unversioned, stale_exempt = judge(rows, EXEMPT_UNVERSIONED)
    if as_json:
        print(
            json.dumps(
                {
                    "modules": rows,
                    "unversioned": sorted(unversioned),
                    "new_unversioned": new_unversioned,
                    "stale_exemptions": stale_exempt,
                    "non_idempotent": [r["file"] for r in nonidem],
                },
                indent=1,
            )
        )
    else:
        print(
            f"table-creating production modules: {len(rows)}, unversioned: {len(unversioned)} "
            f"(exempt: {len(EXEMPT_UNVERSIONED)})"
        )
        for f in new_unversioned:
            print(f"  NEW unversioned store: {f} (open it through apollia_core::schema::open_versioned)")
        for f in stale_exempt:
            print(f"  stale exemption: {f} (no longer an unversioned store; remove it from EXEMPT_UNVERSIONED)")
        for r in nonidem:
            print(f"  non-idempotent DDL: {r['file']}")
            for k, n in r["ddl_not_idempotent"]:
                print(f"      {k} {n} without IF NOT EXISTS")
            if r["alter_add_column"] and not r["alter_probed"]:
                print(f"      {r['alter_add_column']} ALTER ADD COLUMN without probe or tolerance")
    bad = len(new_unversioned) + len(stale_exempt) + len(nonidem)
    print(f"verdict: {'RED' if bad else 'GREEN'} ({len(new_unversioned)} new, {len(stale_exempt)} stale, {len(nonidem)} non-idempotent)")
    return 1 if bad else 0


# ── Selftest ─────────────────────────────────────────────────────────────────

UNVERSIONED_SAMPLE = """\
pub fn open(conn: &Connection) -> Result<(), Error> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS things (id TEXT PRIMARY KEY)")
}
"""

VERSIONED_SAMPLE = """\
const SCHEMA_VERSION: u32 = 1;
pub fn open(conn: &Connection) -> Result<(), Error> {
    apollia_core::schema::open_versioned(conn, "things.db", SCHEMA_VERSION, &[v1])
}
fn v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS things (id TEXT PRIMARY KEY)")
}
"""

NON_IDEMPOTENT_SAMPLE = """\
pub fn open(conn: &Connection) -> Result<(), Error> {
    conn.execute_batch("CREATE TABLE things (id TEXT PRIMARY KEY)")
}
"""


def selftest():
    import tempfile

    failures = []
    with tempfile.TemporaryDirectory() as tmp:
        src = os.path.join(tmp, "crates", "x", "src")
        os.makedirs(src)
        with open(os.path.join(src, "store.rs"), "w") as f:
            f.write(UNVERSIONED_SAMPLE)
        rows = scan(tmp)
        _u, nonidem, new_unversioned, _s = judge(rows, set())
        if not new_unversioned:
            failures.append("an unversioned store outside the exemption list did not fire")
        if nonidem:
            failures.append(f"IF NOT EXISTS DDL flagged as non-idempotent: {nonidem}")

        with open(os.path.join(src, "store.rs"), "w") as f:
            f.write(VERSIONED_SAMPLE)
        rows = scan(tmp)
        _u, nonidem, new_unversioned, _s = judge(rows, set())
        if new_unversioned:
            failures.append(f"a store opened through open_versioned still fired: {new_unversioned}")

        # A stale exemption is a red, not a silent shrink refusal.
        _u, _n, _new, stale = judge(rows, {"no/such/module.rs"})
        if not stale:
            failures.append("a stale exemption did not fire")

        with open(os.path.join(src, "store.rs"), "w") as f:
            f.write(NON_IDEMPOTENT_SAMPLE)
        rows = scan(tmp)
        _u, nonidem, _new, _s = judge(rows, set())
        if not nonidem:
            failures.append("a CREATE TABLE without IF NOT EXISTS did not fire")

    if failures:
        for msg in failures:
            print(f"  FAIL  {msg}")
        print("selftest verdict: RED")
        return 1
    print("  ok    unversioned fires, versioned passes, stale exemption fires, bare DDL fires")
    print("selftest verdict: GREEN")
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--root", default=os.path.join(REPO_ROOT, "crates"))
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--selftest", action="store_true", help="drive the rules on fixtures, red first")
    ns = ap.parse_args()
    if ns.selftest:
        return selftest()
    return run(ns.root, ns.json)


if __name__ == "__main__":
    sys.exit(main())
