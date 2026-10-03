#!/usr/bin/env python3
"""Count the desktop commands that serve a runtime running as a daemon.

The desktop either runs the runtime in its own process or attaches to the
daemon `apollia-os start`. Attached, a command can only work if it reaches the
runtime over the HTTP API, and a command written against the embedded handle
is refused by the gate of `crates/apollia-desktop/src/attached.rs` with a typed
error. Which commands are routed is the list `ROUTED_COMMANDS` of that file;
which commands exist is every `generate_handler!` block of the desktop crate.
A few commands belong to the window rather than to the runtime (quit, restart,
update, the microphones of this machine): they read no runtime state and serve
both modes as they are. They are the list `WINDOW_COMMANDS` of the same file,
each entry a name and the reason it needs no route, and this guard prints them
with their reasons.

This guard classifies each registered command as `routed`, `window` or
`embedded only`, prints the counts, and holds the last on a descending ratchet:

  grew      more commands are embedded only than the ceiling allows. A command
            was added without a route, and the attached mode lost ground.
  stale     fewer are embedded only than the ceiling says. A command was
            routed; the ceiling goes down in the same commit, so the progress
            cannot be spent later by an unrouted addition.
  unknown   a routed or window name no `generate_handler!` block registers:
            the gate would let through a command that does not exist, and the
            count of served commands would be inflated by it.
  reasonless a window command without its reason: the exception would be a
            hole nobody can check.

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
WINDOW_CONST = re.compile(
    r"\bconst\s+WINDOW_COMMANDS\s*:\s*&\[\(&str,\s*&str\)\]\s*=\s*&\[(?P<body>.*?)\];",
    re.DOTALL,
)
WINDOW_ENTRY = re.compile(r'\(\s*"([A-Za-z_]\w*)"\s*,\s*"((?:[^"\\]|\\.)*)"\s*,?\s*\)')

# The ceiling of commands that only serve the embedded runtime. It goes down as
# commands are routed, and never up. Measured at 244 of 249 when the attached
# mode first served the chat and the event bus, then at 229 once it served the
# agents, the tasks, the approvals and the questions an agent asks the user,
# then at 182 once every command whose body is one runtime route went through
# the seam (models, MCP servers, connections, notifications, triggers, tools),
# then at 168 once the commands of the window were named as such, then at 164
# once the profile had its routes, then at 162 once the settings of the chat
# became those of the Apollia agent, then at 155 once the memory could be
# cleared, purged, exported and imported through the daemon, then at 149 once
# the conversations of the chat could be configured, renamed, named, erased and
# linked to a project, then at 146 once the metrics of a conversation were read
# from the daemon and its export and reveal named as window commands, then at
# 141 once the memory was read through its routes.
EMBEDDED_ONLY_CEILING = 141

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


def window_commands(root: Path) -> list[tuple[str, str]]:
    """The `(name, reason)` entries of `WINDOW_COMMANDS`, empty when absent."""
    source = root / ROUTED_SOURCE
    if not source.is_file():
        return []
    match = WINDOW_CONST.search(source.read_text(encoding="utf-8"))
    if match is None:
        return []
    return WINDOW_ENTRY.findall(match.group("body"))


def measure(root: Path, ceiling: int) -> tuple[int, list[str]]:
    """Exit code and report lines for the tree at `root`."""
    entries, unread, _blocks = ipc_callers.registrations(root / RUST_SUBTREE)
    registered = sorted({entry["name"] for entry in entries})
    routed = routed_commands(root)
    lines: list[str] = []
    if not registered:
        return 2, ["check_attached_coverage: no generate_handler! entry read, nothing measured"]
    if routed is None:
        return 2, [
            f"check_attached_coverage: no ROUTED_COMMANDS in {ROUTED_SOURCE}, nothing measured"
        ]

    defects: list[str] = []
    for site in unread:
        defects.append(f"unread registration line: {site}")
    duplicates = sorted({name for name in routed if routed.count(name) > 1})
    for name in duplicates:
        defects.append(f"routed twice: {name}")
    unknown = sorted(set(routed) - set(registered))
    for name in unknown:
        defects.append(f"unknown: {name} is routed but no generate_handler! block registers it")
    window = window_commands(root)
    window_names = [name for name, _ in window]
    for name in sorted({name for name in window_names if window_names.count(name) > 1}):
        defects.append(f"window twice: {name}")
    for name in sorted(set(window_names) & set(routed)):
        defects.append(f"both routed and window: {name}")
    for name in sorted(set(window_names) - set(registered)):
        defects.append(
            f"unknown: {name} is a window command but no generate_handler! block registers it"
        )
    for name, reason in window:
        if not reason.strip():
            defects.append(f"reasonless: the window command {name} gives no reason")
    routed_registered = sorted(set(routed) & set(registered))
    window_registered = sorted(set(window_names) & set(registered))
    served = set(routed) | set(window_names)
    embedded_only = [name for name in registered if name not in served]
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
        f"{len(routed_registered)} routed, {len(window_registered)} window, "
        f"{count} embedded only (ceiling {ceiling})"
    )
    for name, reason in window:
        lines.append(f"  window: {name}, {reason}")
    if defects:
        lines.extend(f"  {defect}" for defect in defects)
        return 1, lines
    lines.append("check_attached_coverage: the attached mode holds its ground")
    return 0, lines


# ── selftest ─────────────────────────────────────────────────────────────────


def _subject(
    root: Path,
    registered: list[str],
    routed: list[str] | None,
    window: list[tuple[str, str]] | None = None,
) -> None:
    src = root / RUST_SUBTREE
    src.mkdir(parents=True, exist_ok=True)
    entries = "".join(f"        commands::{name},\n" for name in registered)
    (src / "handlers.rs").write_text(
        "pub fn commands() {\n    tauri::generate_handler![\n" + entries + "    ]\n}\n",
        encoding="utf-8",
    )
    if routed is not None:
        names = "".join(f'    "{name}",\n' for name in routed)
        pairs = "".join(f'    ("{name}", "{reason}"),\n' for name, reason in window or [])
        (root / ROUTED_SOURCE).write_text(
            f"pub(crate) const ROUTED_COMMANDS: &[&str] = &[\n{names}];\n"
            f"pub(crate) const WINDOW_COMMANDS: &[(&str, &str)] = &[\n{pairs}];\n",
            encoding="utf-8",
        )


def selftest() -> int:
    cases: list[tuple[str, bool]] = []

    def run(
        registered: list[str],
        routed: list[str] | None,
        ceiling: int,
        window: list[tuple[str, str]] | None = None,
    ) -> tuple[int, str]:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            _subject(root, registered, routed, window)
            code, lines = measure(root, ceiling)
            return code, "\n".join(lines)

    code, text = run(["a", "b", "c"], ["a"], 2)
    cases.append(
        (
            "a tree at its ceiling passes",
            code == 0 and "1 routed, 0 window, 2 embedded only" in text,
        )
    )
    code, text = run(["a", "b", "c", "d"], ["a"], 2)
    cases.append(("an unrouted addition is reported as grown", code == 1 and "grew:" in text))
    code, text = run(["a", "b", "c"], ["a", "b"], 2)
    cases.append(
        ("a routed command asks for the ceiling to go down", code == 1 and "stale:" in text)
    )
    code, text = run(["a", "b", "c"], ["a", "ghost"], 2)
    cases.append(
        ("a routed name nothing registers is reported", code == 1 and "unknown: ghost" in text)
    )
    code, _ = run(["a", "b"], None, 2)
    cases.append(("a missing ROUTED_COMMANDS measures nothing", code == 2))
    code, _ = run([], ["a"], 0)
    cases.append(("a tree without registrations measures nothing", code == 2))
    code, text = run(["a", "b", "c"], ["a"], 1, [("b", "closes the window")])
    cases.append(
        (
            "a window command is not embedded only and is printed with its reason",
            code == 0 and "1 window, 1 embedded only" in text and "b, closes the window" in text,
        )
    )
    code, text = run(["a", "b"], ["a"], 0, [("b", " ")])
    cases.append(
        ("a window command without a reason is refused", code == 1 and "reasonless: " in text)
    )
    code, text = run(["a", "b"], ["a"], 1, [("ghost", "why")])
    cases.append(
        ("a window name nothing registers is reported", code == 1 and "unknown: ghost" in text)
    )
    code, text = run(["a", "b"], ["a", "b"], 0, [("b", "why")])
    cases.append(
        (
            "a command both routed and window is refused",
            code == 1 and "both routed and window: b" in text,
        )
    )

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
