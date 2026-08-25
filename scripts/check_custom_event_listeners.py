#!/usr/bin/env python3
"""Cross every custom DOM event the desktop front emits against its listeners.

The command palette reaches a route by dispatching a `CustomEvent` rather than
by importing the screen that performs the action. That decoupling is the point,
and it is also the failure: an emission whose listener was never written
compiles, type-checks, passes every unit test, and shows the user a menu entry
that does nothing. Three of them lived in the palette at once, including
"start all agents" and "stop all agents".

Nothing else in the tree crosses the two sides. `check_tauri_ipc_callers.py`
reads the Rust-to-webview contract; this one reads the webview's contract with
itself.

  * emissions, `crates/apollia-desktop/ui/src`: every `new CustomEvent(...)`.
  * listeners, the same subtree: every `addEventListener(...)`.

Both sides accept a literal name or an identifier that resolves to a
module-level string constant. Resolving matters and is not a refinement: the
observability route registers its listener through a `FOCUS_TASK_EVENT`
constant, and a literal-only scan reports that live event as dead.

Verdict by exit code, since the caller reads it rather than the text:

  0  every emitted event has at least one listener
  1  at least one emitted event has none
  2  nothing was measured, so the run says nothing about the tree: the subtree
     is absent, or it holds no emission at all
"""

import argparse
import re
import sys
from pathlib import Path

UI_SRC = Path(__file__).resolve().parent.parent / "crates/apollia-desktop/ui/src"

CONST = re.compile(r'\bconst\s+([A-Za-z_$][\w$]*)\s*(?::\s*[^=]+)?=\s*["\']([^"\']+)["\']')
EMIT = re.compile(r'new\s+CustomEvent\(\s*([^,)]+?)\s*[,)]')
LISTEN = re.compile(r'addEventListener\(\s*([^,)]+?)\s*[,)]')
LITERAL = re.compile(r'^["\']([^"\']+)["\']$')


def resolve(token: str, constants: dict[str, str]) -> str | None:
    """Return the event name a call site names, or None when it is computed."""
    token = token.strip()
    literal = LITERAL.match(token)
    if literal:
        return literal.group(1)
    return constants.get(token)


def main() -> int:
    if not UI_SRC.is_dir():
        print(f"NOTHING MEASURED: {UI_SRC} is absent", file=sys.stderr)
        return 2

    files = [p for p in UI_SRC.rglob("*") if p.suffix in (".ts", ".svelte")]
    emitted: dict[str, list[str]] = {}
    listened: set[str] = set()
    unresolved = 0

    for path in files:
        src = path.read_text(errors="ignore")
        constants = {m.group(1): m.group(2) for m in CONST.finditer(src)}
        rel = path.relative_to(UI_SRC).as_posix()
        for match in EMIT.finditer(src):
            name = resolve(match.group(1), constants)
            if name is None:
                unresolved += 1
                continue
            emitted.setdefault(name, []).append(rel)
        for match in LISTEN.finditer(src):
            name = resolve(match.group(1), constants)
            if name is None:
                unresolved += 1
                continue
            listened.add(name)

    if not emitted:
        print("NOTHING MEASURED: no CustomEvent emission found", file=sys.stderr)
        return 2

    dead = sorted(name for name in emitted if name not in listened)

    print(f"custom events emitted   : {len(emitted)}")
    print(f"events with a listener  : {len(emitted) - len(dead)}")
    print(f"call sites not resolved : {unresolved} (computed name, not a literal or a constant)")

    if dead:
        print()
        for name in dead:
            print(f"  NO LISTENER  {name}")
            for site in emitted[name]:
                print(f"               emitted in {site}")
        print()
        print("An emitted event nobody listens to is an action that looks alive.")
        return 1

    print()
    print("OK: every emitted custom event has a listener")
    return 0


if __name__ == "__main__":
    argparse.ArgumentParser(description=__doc__.splitlines()[0]).parse_args()
    sys.exit(main())
