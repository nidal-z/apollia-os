#!/usr/bin/env python3
"""Cross every custom DOM event the desktop front emits against its listeners.

The command palette reaches a route by dispatching a `CustomEvent` rather than
by importing the screen that performs the action. That decoupling is the point,
and it is also the failure: an emission whose listener was never written
compiles, type-checks, passes every unit test, and shows the user a menu entry
that does nothing. Three of them lived in the palette at once, including
"start all agents" and "stop all agents".

`check_tauri_ipc_callers.py` reads the command contract. This file reads the two
event contracts, which are distinct channels and were both unguarded:

  1. the webview's DOM bus with itself: `new CustomEvent(...)` against
     `addEventListener(...)`, both in `crates/apollia-desktop/ui/src`.
  2. the Tauri application bus between the shell and the webview:
     `app.emit(...)` in `crates/apollia-desktop/src` and `emit(...)` in the
     webview, against `listen(...)` on the other side.

The second crossing was the one nobody read, and it held three defects at once:
`a2a:worker_status` was listened to and emitted by no crate, `hook-decision` and
`todo-updated` were emitted and listened to by nobody. A channel is a name
agreed on two sides and checked by neither compiler.

Both crossings accept a literal name or an identifier that resolves to a
module-level string constant. Resolving matters and is not a refinement: the
observability route registers its listener through a `FOCUS_TASK_EVENT`
constant, the dictation failure travels under `DICTATION_FAILED_EVENT`, and a
literal-only scan reports both live channels as dead.

Verdict by exit code, since the caller reads it rather than the text:

  0  every emitted event has a listener, and every listener an emitter
  1  at least one channel has a single side
  2  nothing was measured, so the run says nothing about the tree: a subtree is
     absent, or it holds no emission at all

Usage:
    python3 scripts/check_custom_event_listeners.py
    python3 scripts/check_custom_event_listeners.py --selftest
"""

import argparse
import contextlib
import io
import re
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
UI_SRC = REPO_ROOT / "crates/apollia-desktop/ui/src"
SHELL_SRC = REPO_ROOT / "crates/apollia-desktop/src"

CONST = re.compile(r'\bconst\s+([A-Za-z_$][\w$]*)\s*(?::\s*[^=]+)?=\s*["\']([^"\']+)["\']')
EMIT = re.compile(r'new\s+CustomEvent\(\s*([^,)]+?)\s*[,)]')
LISTEN = re.compile(r'addEventListener\(\s*([^,)]+?)\s*[,)]')
LITERAL = re.compile(r'^["\']([^"\']+)["\']$')

# Tauri application bus. `emit_to` names its target first, then the channel.
RUST_CONST = re.compile(r'\bconst\s+([A-Za-z_][\w]*)\s*:\s*&(?:\'static\s+)?str\s*=\s*"([^"]+)"')
RUST_EMIT = re.compile(r'\.emit\(\s*([^,)]+?)\s*[,)]', re.S)
RUST_EMIT_TO = re.compile(r'\.emit_to\(\s*[^,]+,\s*([^,)]+?)\s*[,)]', re.S)
RUST_LISTEN = re.compile(r'\.(?:listen|once)\(\s*([^,)]+?)\s*[,)]', re.S)
TS_TAURI_IMPORT = re.compile(r'["\']@tauri-apps/api/event["\']')
TS_EMIT = re.compile(r'(?<![.\w])emit\(\s*([^,)]+?)\s*[,)]', re.S)
TS_LISTEN = re.compile(r'(?<![.\w])(?:listen|once)(?:<[^(]*?>)?\(\s*([^,)]+?)\s*[,)]', re.S)


def resolve(token: str, constants: dict[str, str]) -> str | None:
    """Return the event name a call site names, or None when it is computed."""
    token = token.strip()
    literal = LITERAL.match(token)
    if literal:
        return literal.group(1)
    return constants.get(token)


def blank_rust_comments(text: str) -> str:
    """Blank line comments, so a doc-comment naming a channel is not a call.

    The `//` must not follow a colon: two channels of this tree are named
    `oauth://code-ready` and `oauth://error`, and a comment blanker that reads
    their scheme separator as a comment erases the emission itself.
    """
    return re.sub(r"(?<![:\w])//[^\n]*", lambda m: " " * len(m.group(0)), text)


def rust_production(text: str) -> str:
    """The part of a Rust file that is not its `#[cfg(test)]` module."""
    marker = re.search(r"#\[cfg\(test\)\]\s*(pub\s+)?mod\b", text)
    return text[: marker.start()] if marker else text


def resolve_rust(token: str, constants: dict[str, str]) -> str | None:
    """Return the channel a Rust call site names, or None when it is computed."""
    token = token.strip()
    if token.startswith('"') and token.endswith('"'):
        return token[1:-1]
    # `i18n::EVENT_UI_LOCALE`, `crate::i18n::EVENT_UI_LOCALE`: the constant is
    # the last segment, and the path says nothing the crossing needs.
    return constants.get(token.rsplit("::", 1)[-1])


def scan_tauri_bus(ui_src: Path, shell_src: Path) -> tuple[dict, dict, int]:
    """Both sides of the Tauri application bus, keyed by channel name.

    Returns (emitters, listeners, unresolved), each mapping a channel to the
    call sites that name it.
    """
    emitters: dict[str, list[str]] = {}
    listeners: dict[str, list[str]] = {}
    unresolved = 0

    rust_files = sorted(shell_src.rglob("*.rs")) if shell_src.is_dir() else []
    constants: dict[str, str] = {}
    bodies: list[tuple[str, str]] = []
    for path in rust_files:
        text = rust_production(blank_rust_comments(path.read_text(errors="ignore")))
        constants.update({m.group(1): m.group(2) for m in RUST_CONST.finditer(text)})
        bodies.append((path.relative_to(shell_src.parent.parent).as_posix(), text))
    for rel, text in bodies:
        for pattern, bucket in ((RUST_EMIT, emitters), (RUST_EMIT_TO, emitters), (RUST_LISTEN, listeners)):
            for match in pattern.finditer(text):
                name = resolve_rust(match.group(1), constants)
                if name is None:
                    unresolved += 1
                    continue
                bucket.setdefault(name, []).append(rel)

    ui_files = [p for p in ui_src.rglob("*") if p.suffix in (".ts", ".svelte")] if ui_src.is_dir() else []
    ui_sources = [(p, p.read_text(errors="ignore")) for p in ui_files]
    # One table for the whole subtree, not one per file: the dictation channel
    # is declared in `lib/stt/dictationFailure.ts` and imported by the three
    # components that listen to it, so a per-file table resolves none of them.
    ts_constants: dict[str, str] = {}
    for _path, src in ui_sources:
        ts_constants.update({m.group(1): m.group(2) for m in CONST.finditer(src)})
    for path, src in ui_sources:
        # Only files that reach for the Tauri event API speak on this bus: a
        # local `emit(preset)` helper in a component is not a channel. The
        # import is sometimes dynamic (`await import("@tauri-apps/api/event")`),
        # so the module specifier is what is looked for, not the `from` form.
        if not TS_TAURI_IMPORT.search(src):
            continue
        rel = path.relative_to(ui_src).as_posix()
        for pattern, bucket in ((TS_EMIT, emitters), (TS_LISTEN, listeners)):
            for match in pattern.finditer(src):
                name = resolve(match.group(1), ts_constants)
                if name is None:
                    unresolved += 1
                    continue
                bucket.setdefault(name, []).append(rel)
    return emitters, listeners, unresolved


def report_tauri_bus(ui_src: Path, shell_src: Path) -> int:
    emitters, listeners, unresolved = scan_tauri_bus(ui_src, shell_src)
    if not emitters:
        print("NOTHING MEASURED: no Tauri channel emission found", file=sys.stderr)
        return 2
    channels = sorted(set(emitters) | set(listeners))
    no_listener = sorted(n for n in emitters if n not in listeners)
    no_emitter = sorted(n for n in listeners if n not in emitters)

    print(f"Tauri channels          : {len(channels)}")
    print(f"channels with two sides : {len(channels) - len(no_listener) - len(no_emitter)}")
    print(f"call sites not resolved : {unresolved} (computed name, not a literal or a constant)")

    if no_listener or no_emitter:
        print()
        for name in no_listener:
            print(f"  NO LISTENER  {name}")
            for site in sorted(set(emitters[name])):
                print(f"               emitted in {site}")
        for name in no_emitter:
            print(f"  NO EMITTER   {name}")
            for site in sorted(set(listeners[name])):
                print(f"               listened in {site}")
        print()
        print("A channel with one side is a name two files agreed on and no compiler checks.")
        return 1

    print()
    print("OK: every Tauri channel has an emitter and a listener")
    return 0


def report_dom_bus() -> int:
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


def _case(label: str, ok: bool) -> bool:
    print(f"  {'ok  ' if ok else 'FAIL'}  {label}")
    return ok


def _write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def _tauri_subject(tmp: Path, *, orphan: bool) -> tuple[Path, Path]:
    """A miniature shell and webview, with the shapes that fooled a plain grep."""
    ui = tmp / "ui"
    shell = tmp / "crates/apollia-desktop/src"
    _write(
        shell / "events.rs",
        'const DICTATION_FAILED_EVENT: &str = "stt-dictation-failed";\n'
        '/// Doc-comment naming `app.emit("never-emitted", …)`, which is prose.\n'
        'fn go(app: &AppHandle) {\n'
        '    let _ = app.emit("runtime-event", &payload);\n'
        '    let _ = app.emit(DICTATION_FAILED_EVENT, &payload);\n'
        + ('    let _ = app.emit("orphan-channel", &payload);\n' if orphan else "")
        + '}\n'
        '#[cfg(test)]\n'
        'mod tests {\n'
        '    fn t(app: &AppHandle) { let _ = app.emit("test-only-channel", ()); }\n'
        '}\n',
    )
    _write(
        ui / "store.ts",
        'import { listen } from "@tauri-apps/api/event";\n'
        'const DICTATION_FAILED_EVENT = "stt-dictation-failed";\n'
        'void listen<Envelope>("runtime-event", () => {});\n'
        'void listen(DICTATION_FAILED_EVENT, () => {});\n',
    )
    _write(
        ui / "CronBuilder.svelte",
        "<script lang=\"ts\">\n  function emit(p: Preset) { apply(p); }\n  emit(\"daily\");\n</script>\n",
    )
    return ui, shell


def selftest() -> int:
    print("event channels: both directions on a built subject")
    results: list[bool] = []
    with tempfile.TemporaryDirectory(prefix="check-events-") as tmp:
        ui, shell = _tauri_subject(Path(tmp), orphan=True)
        buffer = io.StringIO()
        with contextlib.redirect_stdout(buffer):
            code = report_tauri_bus(ui, shell)
        text = buffer.getvalue()
        results.append(_case("a channel nobody listens to is a red, and it is named", code == 1 and "orphan-channel" in text))
        results.append(_case("a channel named only in a doc-comment is not an emission", "never-emitted" not in text))
        results.append(_case("a channel emitted only under #[cfg(test)] is not an emission", "test-only-channel" not in text))
    with tempfile.TemporaryDirectory(prefix="check-events-") as tmp:
        ui, shell = _tauri_subject(Path(tmp), orphan=False)
        buffer = io.StringIO()
        with contextlib.redirect_stdout(buffer):
            code = report_tauri_bus(ui, shell)
        text = buffer.getvalue()
        results.append(_case("positive control: the same tree without the orphan is green", code == 0))
        results.append(_case("a constant on both sides resolves to one channel", "stt-dictation-failed" not in text))
        results.append(_case("a local emit() helper outside the Tauri API is not a channel", "daily" not in text))
        buffer = io.StringIO()
        with contextlib.redirect_stdout(buffer):
            code = report_tauri_bus(ui, Path(tmp) / "absent")
        results.append(_case("an absent shell measures nothing", code == 2))
    print()
    if all(results):
        print(f"self-test: all {len(results)} cases pass")
        return 0
    print(f"self-test: {results.count(False)} of {len(results)} cases fail")
    return 1


def main() -> int:
    print("== webview DOM bus")
    dom = report_dom_bus()
    print()
    print("== Tauri application bus")
    tauri = report_tauri_bus(UI_SRC, SHELL_SRC)
    if 2 in (dom, tauri):
        return 2
    return 1 if (dom or tauri) else 0


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--selftest", action="store_true", help="check the guard itself against a built subject"
    )
    args = parser.parse_args()
    sys.exit(selftest() if args.selftest else main())
