#!/usr/bin/env python3
"""Every child process spawned from the desktop must ask for no console window.

Why this exists as a check and not as a paragraph in the rulebook. The desktop
binary is built for the Windows GUI subsystem, so it owns no console; when it
starts a console-subsystem executable, Windows allocates and shows a brand new
console for the child. One sweep put `CREATE_NO_WINDOW` on ten spawn sites and
listed four crates left to do. Nobody picked them up, and a later change added
three fresh sites in files that did not exist when the sweep ran. The count went
up, not down, and no gate noticed. A written rule did not survive two weeks.

The check: in every crate reachable from the desktop binary, each production
`Command::new(` must be followed, within a short window of lines, by a call to
`apollia_core::subprocess_window::hide_console` or its async twin.

Two exemptions, both narrow. Test code: a test that spawns `/bin/sh` shows
nothing to a user. And a spawn inside a function gated to a platform that is not
Windows, `#[cfg(target_os = "linux")]` and friends, since the flag is a no-op
there by construction. A function with no gate, or gated in a way that still
compiles on Windows, is in scope.

Run: python3 scripts/check_subprocess_window.py
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

# Crates whose code can run inside the desktop process. A spawn from anywhere
# here is a spawn the operator can see.
SCANNED_CRATES = [
    "apollia-core",
    "apollia-desktop",
    "apollia-runtime",
    "apollia-tools",
    "apollia-workspace",
    "apollia-mcp",
    "apollia-connectors",
    "apollia-auth",
    "apollia-stt",
]

SPAWN = re.compile(r"\bCommand::new\s*\(")
HIDE = re.compile(r"\bhide_console(_async)?\s*\(")
# The helper is allowed to name the flag it applies, and the check itself must
# not flag the module that implements it.
EXEMPT_FILES = {"subprocess_window.rs"}
# Lines between the spawn and the hide. Enough for the builder calls that
# usually sit between the two, tight enough that an unrelated later call cannot
# satisfy the rule by accident.
WINDOW = 14


# Attribute payloads that keep a function off Windows. Matched on the enclosing
# `fn`, not on the spawn line, because that is where the gate is written.
NON_WINDOWS_CFG = re.compile(
    r'#\[cfg\(.*(target_os\s*=\s*"(linux|macos|ios|android|freebsd)"|\bunix\b|target_family\s*=\s*"unix").*\)\]'
)
FN_DECL = re.compile(r"^\s*(pub(\([^)]*\))?\s+)?(async\s+)?(unsafe\s+)?fn\s")


def is_off_windows(lines: list[str], index: int) -> bool:
    """True when the spawn is compiled out on Windows.

    A platform gate is written in one of two places: as an attribute on the
    enclosing function, or as an attribute on an inner block, which is how a
    function that branches per OS is written here. Both are covered by taking
    the nearest `#[cfg(...)]` above the spawn, bounded by the enclosing `fn` and
    its own attribute lines.

    `#[cfg(not(unix))]` must NOT match: that is precisely the Windows branch,
    and it is checked first. `#[cfg(all(unix, not(target_os = "linux")))]` must
    match: it also carries a negation, yet it is still a Unix-only gate.
    """
    fn_line = 0
    for i in range(index, -1, -1):
        if FN_DECL.search(lines[i]):
            fn_line = i
            break

    for i in range(index - 1, max(fn_line - 8, -1) - 1, -1):
        stripped = lines[i].strip()
        if not stripped.startswith("#[cfg("):
            continue
        if re.search(r"not\s*\(\s*unix\s*\)", stripped):
            return False
        if stripped.startswith("#[cfg(test)]"):
            continue
        return bool(NON_WINDOWS_CFG.search(stripped))
    return False


def is_test_line(lines: list[str], index: int) -> bool:
    """True when the line sits inside a `#[cfg(test)]` block.

    Scans backwards for the nearest `#[cfg(test)]` and, crucially, does not stop
    at the first one found in the file: production code often follows a test
    module in a submodule layout. The marker only applies when no closing of the
    module has intervened, which is approximated by comparing indentation depth.
    """
    depth_at_line = len(lines[index]) - len(lines[index].lstrip())
    for i in range(index - 1, -1, -1):
        stripped = lines[i].strip()
        if stripped.startswith("#[cfg(test)]"):
            marker_depth = len(lines[i]) - len(lines[i].lstrip())
            if marker_depth < depth_at_line:
                return True
        if stripped.startswith("#[test]") or stripped.startswith("#[tokio::test]"):
            return True
    return False


def scan() -> tuple[list[str], int, int]:
    failures: list[str] = []
    spawn_count = 0
    file_count = 0

    for crate in SCANNED_CRATES:
        crate_src = REPO_ROOT / "crates" / crate / "src"
        if not crate_src.is_dir():
            continue
        for path in sorted(crate_src.rglob("*.rs")):
            if path.name in EXEMPT_FILES:
                continue
            try:
                lines = path.read_text(encoding="utf-8", errors="ignore").splitlines()
            except OSError:
                continue
            file_count += 1
            for index, line in enumerate(lines):
                if not SPAWN.search(line):
                    continue
                if is_test_line(lines, index) or is_off_windows(lines, index):
                    continue
                spawn_count += 1
                window = "\n".join(lines[index : index + WINDOW])
                if HIDE.search(window):
                    continue
                rel = path.relative_to(REPO_ROOT)
                failures.append(f"{rel}:{index + 1}: {line.strip()}")

    return failures, spawn_count, file_count


def main() -> int:
    failures, spawn_count, file_count = scan()

    print(
        f"{spawn_count} production spawn site(s) in {file_count} file(s) "
        f"across {len(SCANNED_CRATES)} crate(s)"
    )

    if not failures:
        print("every one of them asks for no console window")
        return 0

    print(
        f"\n{len(failures)} spawn site(s) would open a console window on Windows.\n"
        "Call apollia_core::subprocess_window::hide_console (or hide_console_async\n"
        "for a tokio Command) right before the spawn. On every platform but Windows\n"
        "it is a no-op, so no cfg is needed at the call site.\n",
        file=sys.stderr,
    )
    for failure in failures:
        print(f"  {failure}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    argparse.ArgumentParser(description=__doc__.splitlines()[0]).parse_args()
    sys.exit(main())
