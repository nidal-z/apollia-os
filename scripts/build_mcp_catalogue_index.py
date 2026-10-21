#!/usr/bin/env python3
"""Gather the connection catalogue into the index the application reads online.

The catalogue has one source: the selection compiled into the application,
`crates/apollia-desktop/src/mcp/enrichments.json`, and the npm lock of each
pinned npm entry under `crates/apollia-desktop/src/mcp/npm_server/locks/`.
This script writes them as one index: every entry of the selection, in file
order, and on each pinned npm entry the lock it is installed from, under
`npm_lock`. The registry workflow signs the output in the minisign format with
the distribution key (`cargo tauri signer sign`) and publishes the index and
its signature side by side. The application accepts no other key, and lays
the entries of the index over the selection it was built with, entry by
entry.

Editing the catalogue is editing that source: an entry added, withdrawn or
reclassified there reaches every installed application at the next
publication, without a release.

The index is deterministic: keys in file order, two spaces, one trailing
newline. Two runs over the same tree give the same bytes.

Usage:
    python3 scripts/build_mcp_catalogue_index.py [--out PATH]
    python3 scripts/build_mcp_catalogue_index.py --test-fixture

`--test-fixture` rewrites the fixture the Rust tests verify: the index built
from the tree, signed by a throwaway key made on the spot with
`cargo tauri signer generate`, whose private half is deleted with its
temporary directory. Only the public half is kept beside the fixture.
"""

import argparse
import json
import pathlib
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
MCP = ROOT / "crates" / "apollia-desktop" / "src" / "mcp"
SELECTION = MCP / "enrichments.json"
LOCKS = MCP / "npm_server" / "locks"
FIXTURES = MCP / "remote_catalogue" / "fixtures"
FIXTURE_INDEX = FIXTURES / "index.json"
FIXTURE_SIGNATURE = FIXTURES / "index.json.minisig"
FIXTURE_KEY = FIXTURES / "test-dist.key.pub"

SCHEMA_VERSION = 1


class Refused(Exception):
    """The source cannot make a valid index."""


def locks_by_package() -> dict:
    """The npm lock of each package, by the one dependency its root names."""
    out = {}
    for path in sorted(LOCKS.glob("*/package-lock.json")):
        lock = json.loads(path.read_text(encoding="utf-8"))
        names = list(lock.get("packages", {}).get("", {}).get("dependencies", {}))
        if len(names) != 1:
            raise Refused(f"{path.relative_to(ROOT)}: the root names {len(names)} packages, not one")
        out[names[0]] = lock
    return out


def build() -> str:
    selection = json.loads(SELECTION.read_text(encoding="utf-8"))
    locks = locks_by_package()
    entries = []
    for entry in selection:
        package = entry["package_identifier"]
        pinned = entry.get("package_version")
        npm = entry.get("package_registry_type") == "npm"
        if npm and pinned and not entry.get("withdrawn"):
            lock = locks.get(package)
            if lock is None:
                raise Refused(f"{package} is pinned at {pinned} and has no lock")
            locked = lock["packages"][""]["dependencies"][package]
            if locked != pinned:
                raise Refused(f"{package} is pinned at {pinned}, its lock names {locked}")
            entry = {**entry, "npm_lock": lock}
        entries.append(entry)
    index = {"schema_version": SCHEMA_VERSION, "entries": entries}
    return json.dumps(index, ensure_ascii=False, indent=2) + "\n"


def write_test_fixture() -> int:
    """Rebuild the fixture index and sign it with a key made for the occasion."""
    FIXTURES.mkdir(parents=True, exist_ok=True)
    FIXTURE_INDEX.write_text(build(), encoding="utf-8")
    with tempfile.TemporaryDirectory() as tmp:
        key = pathlib.Path(tmp) / "catalogue-test.key"
        password = "catalogue-test"
        subprocess.run(
            ["cargo", "tauri", "signer", "generate", "--ci", "-p", password, "-w", str(key)],
            check=True,
            capture_output=True,
        )
        subprocess.run(
            ["cargo", "tauri", "signer", "sign", "-f", str(key), "-p", password, str(FIXTURE_INDEX)],
            check=True,
            capture_output=True,
        )
        signature = FIXTURE_INDEX.with_name(FIXTURE_INDEX.name + ".sig")
        signature.replace(FIXTURE_SIGNATURE)
        public = pathlib.Path(str(key) + ".pub").read_text(encoding="utf-8")
        FIXTURE_KEY.write_text(public.strip() + "\n", encoding="utf-8")
    sys.stdout.write(f"build_mcp_catalogue_index: fixture rewritten under {FIXTURES.relative_to(ROOT)}\n")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--out", type=pathlib.Path)
    parser.add_argument("--test-fixture", action="store_true")
    args = parser.parse_args()
    try:
        if args.test_fixture:
            return write_test_fixture()
        text = build()
    except Refused as refused:
        sys.stderr.write(f"build_mcp_catalogue_index: refused: {refused}\n")
        return 1
    if args.out:
        args.out.write_text(text, encoding="utf-8")
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
