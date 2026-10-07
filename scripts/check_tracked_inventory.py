#!/usr/bin/env python3
"""Refuse a tracked file that is empty, or a tracked binary executable.

Two files reached the tracked tree that no review would have accepted had it
seen them: an empty file named after a shell at the repository root, left by a
redirection that went wrong, and a 54 MB Windows executable staged beside the
desktop sources for a local build. Neither is visible in a diff: an empty file
shows no line, and a binary shows no content. This guard reads the inventory
instead of the diff.

Rules, each measured on every regular file of the index:

  empty        the blob has zero bytes, and its path is not in `ALLOWED_EMPTY`
  executable   the blob starts with the magic number of an ELF, PE, Mach-O or
               WebAssembly binary, or its name carries the extension of one,
               and its path is not in `ALLOWED_BINARIES`
  stale        an allowed path is no longer tracked, or no longer needs its
               exemption: the list can only describe the tree as it is

The inventory is the index, read through `git ls-files -s` and
`git cat-file --batch`, never the disk: the verdict is the same in a working
copy carrying build output and in a fresh extraction of the same commit, and
a file staged for the next commit is judged before it lands.

Symbolic links and submodule entries are not regular files and are not read.

Exit codes:
  0  no tracked file is empty or a binary executable outside the lists.
  1  at least one finding, or a stale exemption.
  2  nothing measured: not a git work tree, or an index with no regular file.

Usage:
    python3 scripts/check_tracked_inventory.py
    python3 scripts/check_tracked_inventory.py --selftest
"""

import argparse
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

# Empty on purpose. Each entry names the reader that gives the empty file its
# meaning; an entry without one is the defect this guard exists to stop.
ALLOWED_EMPTY = {
    # Keeps the `python/**/*` resource glob of tauri.conf.json matching in a
    # tree where no interpreter has been staged.
    "crates/apollia-desktop/python/.gitkeep",
    # Read by `include_str!` in crates/apollia-signing/src/distribution.rs:
    # empty means a build that carries no distribution key.
    "crates/apollia-signing/keys/distribution.key.pub",
    # Tells the static host not to run the published site through Jekyll.
    "docs/site/static/.nojekyll",
    # PEP 561 marker: its presence, not its content, declares the SDK typed.
    "sdk/apollia/py.typed",
}

# Binary executables tracked on purpose. None today: the tree is built from
# source, and every binary a package carries is fetched or compiled at build
# time.
ALLOWED_BINARIES: set[str] = set()

REGULAR_MODES = {"100644", "100755"}

MAGIC = (
    (b"\x7fELF", "ELF"),
    (b"MZ", "PE"),
    (b"\xfe\xed\xfa\xce", "Mach-O"),
    (b"\xfe\xed\xfa\xcf", "Mach-O"),
    (b"\xce\xfa\xed\xfe", "Mach-O"),
    (b"\xcf\xfa\xed\xfe", "Mach-O"),
    (b"\xca\xfe\xba\xbe", "Mach-O universal"),
    (b"\x00asm", "WebAssembly"),
)
BINARY_SUFFIXES = (".exe", ".dll", ".so", ".dylib", ".node", ".wasm")

HEAD_BYTES = 4


def binary_kind(path: str, head: bytes) -> str | None:
    """The kind of binary executable `path` is, or None."""
    for magic, kind in MAGIC:
        if head.startswith(magic):
            return kind
    lowered = path.lower()
    for suffix in BINARY_SUFFIXES:
        if lowered.endswith(suffix):
            return f"{suffix} file"
    return None


def read_index(root: Path) -> list[tuple[str, int, bytes]] | None:
    """Path, size and first bytes of every regular file of the index of `root`.

    None when `root` is not a git work tree.
    """
    listing = subprocess.run(
        ["git", "ls-files", "-s", "-z"], cwd=root, capture_output=True, check=False
    )
    if listing.returncode != 0:
        return None
    entries: list[tuple[str, str]] = []
    for record in listing.stdout.split(b"\0"):
        if not record:
            continue
        meta, _, raw_path = record.partition(b"\t")
        mode, sha, _stage = meta.decode("ascii").split()
        if mode in REGULAR_MODES:
            entries.append((sha, raw_path.decode("utf-8", errors="surrogateescape")))
    if not entries:
        return []
    blobs = subprocess.run(
        ["git", "cat-file", "--batch"],
        cwd=root,
        input="".join(f"{sha}\n" for sha, _ in entries).encode("ascii"),
        capture_output=True,
        check=True,
    ).stdout
    files: list[tuple[str, int, bytes]] = []
    cursor = 0
    for _, path in entries:
        newline = blobs.index(b"\n", cursor)
        size = int(blobs[cursor:newline].split()[2])
        start = newline + 1
        files.append((path, size, blobs[start : start + min(size, HEAD_BYTES)]))
        cursor = start + size + 1
    return files


def measure(
    root: Path, allowed_empty: set[str], allowed_binaries: set[str]
) -> tuple[int, list[str]]:
    """Exit code and report lines for the index of `root`."""
    files = read_index(root)
    if files is None:
        return 2, ["check_tracked_inventory: not a git work tree, nothing measured"]
    if not files:
        return 2, ["check_tracked_inventory: the index holds no regular file, nothing measured"]

    findings: list[str] = []
    empty_seen: set[str] = set()
    binaries_seen: set[str] = set()
    for path, size, head in files:
        if size == 0:
            empty_seen.add(path)
            if path not in allowed_empty:
                findings.append(
                    f"  empty: {path} is tracked with zero bytes; remove it, or name "
                    "its reader in ALLOWED_EMPTY"
                )
            continue
        kind = binary_kind(path, head)
        if kind is not None:
            binaries_seen.add(path)
            if path not in allowed_binaries:
                findings.append(
                    f"  executable: {path} is a tracked binary ({kind}, {size} bytes); "
                    "untrack it and ignore its path, or name it in ALLOWED_BINARIES"
                )
    for path in sorted(allowed_empty - empty_seen):
        findings.append(
            f"  stale: {path} is in ALLOWED_EMPTY and is not an empty tracked file; "
            "remove the entry"
        )
    for path in sorted(allowed_binaries - binaries_seen):
        findings.append(
            f"  stale: {path} is in ALLOWED_BINARIES and is not a tracked binary; "
            "remove the entry"
        )

    line = (
        f"check_tracked_inventory: {len(files)} tracked files read, "
        f"{len(empty_seen)} empty ({len(allowed_empty & empty_seen)} allowed), "
        f"{len(binaries_seen)} binary executable(s) "
        f"({len(allowed_binaries & binaries_seen)} allowed)"
    )
    if findings:
        return 1, [line, *findings]
    return 0, [line, "check_tracked_inventory: no empty file and no binary executable outside the lists"]


# ── selftest ─────────────────────────────────────────────────────────────────


def _repo(root: Path, files: dict[str, bytes]) -> Path:
    """A throwaway repository whose index holds exactly `files`."""
    subprocess.run(["git", "init", "-q", str(root)], check=True, capture_output=True)
    for name, content in files.items():
        target = root / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(content)
    subprocess.run(["git", "add", "-A"], cwd=root, check=True, capture_output=True)
    return root


def selftest() -> int:
    cases: list[tuple[str, bool]] = []
    text = {"README.md": b"# A tree\n", "src/main.rs": b"fn main() {}\n"}

    def run(
        files: dict[str, bytes],
        allowed_empty: set[str] | None = None,
        allowed_binaries: set[str] | None = None,
    ) -> tuple[int, str]:
        # GIVEN a throwaway repository whose index holds `files`
        with tempfile.TemporaryDirectory() as tmp:
            root = _repo(Path(tmp) / "repo", files)
            # WHEN the inventory is measured
            code, lines = measure(root, allowed_empty or set(), allowed_binaries or set())
            return code, "\n".join(lines)

    # THEN each verdict below holds
    code, report = run(text)
    cases.append(("a tree of text files passes", code == 0 and "2 tracked files read" in report))

    code, report = run({**text, "bash": b""})
    cases.append(("an empty tracked file is refused by name", code == 1 and "empty: bash " in report))

    code, report = run({**text, "app/tool.exe": b"MZ\x90\x00" + b"\0" * 60})
    cases.append(
        ("a Windows executable is refused by name", code == 1 and "executable: app/tool.exe " in report)
    )

    code, report = run({**text, "bin/tool": b"\x7fELF\x02\x01\x01" + b"\0" * 60})
    cases.append(
        ("an ELF binary without extension is refused on its magic number", code == 1 and "(ELF," in report)
    )

    code, report = run({**text, "bin/tool": b"\xcf\xfa\xed\xfe" + b"\0" * 60})
    cases.append(("a Mach-O binary is refused on its magic number", code == 1 and "(Mach-O," in report))

    code, report = run({**text, "lib/native.dylib": b"not a real library\n"})
    cases.append(("a binary extension is refused whatever the content", code == 1 and ".dylib file" in report))

    code, _ = run({**text, "pkg/py.typed": b""}, allowed_empty={"pkg/py.typed"})
    cases.append(("an allowed empty file passes", code == 0))

    code, _ = run({**text, "vendor/tool.exe": b"MZ\x90\x00"}, allowed_binaries={"vendor/tool.exe"})
    cases.append(("an allowed binary passes", code == 0))

    code, report = run({**text, "pkg/py.typed": b"partial\n"}, allowed_empty={"pkg/py.typed"})
    cases.append(("an allowed path that is no longer empty is stale", code == 1 and "stale: pkg/py.typed" in report))

    code, report = run(text, allowed_binaries={"vendor/gone.exe"})
    cases.append(("an allowed path that is no longer tracked is stale", code == 1 and "stale: vendor/gone.exe" in report))

    code, report = run({**text, "script.sh": b"#!/bin/sh\necho ok\n", "logo.png": b"\x89PNG\r\n\x1a\n"})
    cases.append(("a script and an image are not executables", code == 0))

    with tempfile.TemporaryDirectory() as tmp:
        # GIVEN a file staged in the index and then deleted from the disk
        root = _repo(Path(tmp) / "repo", {**text, "bash": b""})
        (root / "bash").unlink()
        # WHEN the inventory is measured
        code, lines = measure(root, set(), set())
        # THEN the index is what was read
        cases.append(("the index is read, not the disk", code == 1 and "empty: bash " in "\n".join(lines)))

    with tempfile.TemporaryDirectory() as tmp:
        # GIVEN a directory that is not a repository, then an empty repository
        plain = Path(tmp) / "plain"
        plain.mkdir()
        # WHEN each is measured
        outside, _ = measure(plain, set(), set())
        hollow, _ = measure(_repo(Path(tmp) / "hollow", {}), set(), set())
        # THEN neither is reported as a pass
        cases.append(("a directory outside any repository measures nothing", outside == 2))
        cases.append(("an index without a file measures nothing", hollow == 2))

    failed = [name for name, held in cases if not held]
    for name in failed:
        print(f"selftest FAILED: {name}")
    if failed:
        return 1
    print(f"check_tracked_inventory --selftest: {len(cases)} assertions, every one holds")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Refuse a tracked file that is empty, or a tracked binary executable."
    )
    parser.add_argument("--selftest", action="store_true", help="check the guard against fixtures")
    args = parser.parse_args()
    if args.selftest:
        return selftest()
    code, lines = measure(REPO_ROOT, ALLOWED_EMPTY, ALLOWED_BINARIES)
    for line in lines:
        print(line)
    return code


if __name__ == "__main__":
    sys.exit(main())
