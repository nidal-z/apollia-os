#!/usr/bin/env python3
"""Count the desktop commands that serve a runtime running as a daemon.

The desktop either runs the runtime in its own process or attaches to the
daemon `apollia-os start`. Attached, a command can only work if it reaches the
runtime over the HTTP API, and a command written against the embedded handle
is refused by the gate of `crates/apollia-desktop/src/attached.rs` with a typed
error. Which commands are routed is the list `ROUTED_COMMANDS` of that file;
which commands exist is every `generate_handler!` block of the desktop crate.

This guard classifies each registered command as `routed` or `embedded only`,
prints the two counts, and holds the second on a descending ratchet:

  grew      more commands are embedded only than the ceiling allows. A command
            was added without a route, and the attached mode lost ground.
  stale     fewer are embedded only than the ceiling says. A command was
            routed; the ceiling goes down in the same commit, so the progress
            cannot be spent later by an unrouted addition.
  unknown   a routed name no `generate_handler!` block registers: the gate
            would let through a command that does not exist, and the count of
            routed commands would be inflated by it.

Exit codes:
  0  the counts match the ceiling and every routed name is registered.
  1  the ratchet moved without the ceiling, or a routed name is unknown.
  2  nothing measured: no registration read, or no `ROUTED_COMMANDS` found.

Usage:
    python3 scripts/check_attached_coverage.py
    python3 scripts/check_attached_coverage.py --selftest
"""

import argparse
import re
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import check_tauri_ipc_callers as ipc_callers  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parent.parent
RUST_SUBTREE = Path("crates/apollia-desktop/src")
ROUTED_SOURCE = Path("crates/apollia-desktop/src/attached.rs")

# The ceiling of commands that only serve the embedded runtime. It goes down as
# commands are routed, and never up. Measured at 243 of 248 when the attached
# mode first served the chat and the event bus.
EMBEDDED_ONLY_CEILING = 243

ROUTED_CONST = re.compile(
    r"\bconst\s+ROUTED_COMMANDS\s*:\s*&\[&str\]\s*=\s*&\[(?P<body>.*?)\];",
    re.DOTALL,
)
STRING_LITERAL = re.compile(r'"([A-Za-z_]\w*)"')


def routed_commands(root: Path) -> list[str] | None:
    """The names of `ROUTED_COMMANDS`, `None` when the constant is absent."""
    source = root / ROUTED_SOURCE
    if not source.is_file():
        return None
    match = ROUTED_CONST.search(source.read_text(encoding="utf-8"))
    if match is None:
        return None
    return STRING_LITERAL.findall(match.group("body"))


def measure(root: Path, ceiling: int) -> tuple[int, list[str]]:
    """Exit code and report lines for the tree at `root`."""
    entries, unread, _blocks = ipc_callers.registrations(root / RUST_SUBTREE)
    registered = sorted({entry["name"] for entry in entries})
    routed = routed_commands(root)
    lines: list[str] = []
    if not registered:
        return 2, ["check_attached_coverage: no generate_handler! entry read, nothing measured"]
    if routed is None:
        return 2, [f"check_attached_coverage: no ROUTED_COMMANDS in {ROUTED_SOURCE}, nothing measured"]

    defects: list[str] = []
    for site in unread:
        defects.append(f"unread registration line: {site}")
    duplicates = sorted({name for name in routed if routed.count(name) > 1})
    for name in duplicates:
        defects.append(f"routed twice: {name}")
    unknown = sorted(set(routed) - set(registered))
    for name in unknown:
        defects.append(f"unknown: {name} is routed but no generate_handler! block registers it")
    routed_registered = sorted(set(routed) & set(registered))
    embedded_only = [name for name in registered if name not in set(routed)]
    count = len(embedded_only)
    if count > ceiling:
        defects.append(
            f"grew: {count} commands are embedded only, the ceiling is {ceiling}; "
            "route the new command or explain why it cannot be"
        )
    elif count < ceiling:
        defects.append(
            f"stale: {count} commands are embedded only, the ceiling is still {ceiling}; "
            f"lower EMBEDDED_ONLY_CEILING to {count} in this commit"
        )

    lines.append(
        f"check_attached_coverage: {len(registered)} registered commands, "
        f"{len(routed_registered)} routed, {count} embedded only (ceiling {ceiling})"
    )
    if defects:
        lines.extend(f"  {defect}" for defect in defects)
        return 1, lines
    lines.append("check_attached_coverage: the attached mode holds its ground")
    return 0, lines


# ── selftest ─────────────────────────────────────────────────────────────────


def _subject(root: Path, registered: list[str], routed: list[str] | None) -> None:
    src = root / RUST_SUBTREE
    src.mkdir(parents=True, exist_ok=True)
    entries = "".join(f"        commands::{name},\n" for name in registered)
    (src / "handlers.rs").write_text(
        "pub fn commands() {\n    tauri::generate_handler![\n" + entries + "    ]\n}\n",
        encoding="utf-8",
    )
    if routed is not None:
        names = "".join(f'    "{name}",\n' for name in routed)
        (root / ROUTED_SOURCE).write_text(
            f"pub(crate) const ROUTED_COMMANDS: &[&str] = &[\n{names}];\n",
            encoding="utf-8",
        )


def selftest() -> int:
    cases: list[tuple[str, bool]] = []

    def run(registered: list[str], routed: list[str] | None, ceiling: int) -> tuple[int, str]:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            _subject(root, registered, routed)
            code, lines = measure(root, ceiling)
            return code, "\n".join(lines)

    code, text = run(["a", "b", "c"], ["a"], 2)
    cases.append(("a tree at its ceiling passes", code == 0 and "1 routed, 2 embedded only" in text))
    code, text = run(["a", "b", "c", "d"], ["a"], 2)
    cases.append(("an unrouted addition is reported as grown", code == 1 and "grew:" in text))
    code, text = run(["a", "b", "c"], ["a", "b"], 2)
    cases.append(("a routed command asks for the ceiling to go down", code == 1 and "stale:" in text))
    code, text = run(["a", "b", "c"], ["a", "ghost"], 2)
    cases.append(("a routed name nothing registers is reported", code == 1 and "unknown: ghost" in text))
    code, _ = run(["a", "b"], None, 2)
    cases.append(("a missing ROUTED_COMMANDS measures nothing", code == 2))
    code, _ = run([], ["a"], 0)
    cases.append(("a tree without registrations measures nothing", code == 2))

    failed = [name for name, held in cases if not held]
    for name in failed:
        print(f"selftest FAILED: {name}")
    if failed:
        return 1
    print(f"check_attached_coverage --selftest: {len(cases)} assertions, every one holds")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Classify every desktop command as routed or embedded only, and hold "
            "the embedded-only count on a descending ratchet."
        )
    )
    parser.add_argument("--selftest", action="store_true", help="check the guard against fixtures")
    args = parser.parse_args()
    if args.selftest:
        return selftest()
    code, lines = measure(REPO_ROOT, EMBEDDED_ONLY_CEILING)
    for line in lines:
        print(line)
    return code


if __name__ == "__main__":
    sys.exit(main())
