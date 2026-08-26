#!/usr/bin/env python3
"""Cross every `RuntimeEvent` variant against the code that emits it and the
code that reads it.

`RuntimeEvent` is the runtime's single event catalogue, and the desktop bridge
relays all of it in one exhaustive `match`. That relay is why the catalogue can
rot without anyone noticing: a variant nobody constructs, and a variant nobody
reads, both look wired to the compiler and to a plain grep, because the bridge
mentions every one of them.

Three ways of reading that relay wrong were measured on this tree, and the
counts they produced were wrong in the direction that reports work to do:

  * the relay's dedicated fast paths (`emit_todo_fastpath`, `emit_stt_fastpath`,
    `emit_hitl_fs_fastpath`, `emit_hook_decision_fastpath`, `coalesce_step`) sit
    outside `categorize` and `extract_variant_name`, so excluding those two
    functions alone still let the relay pass for a reader;
  * a guarded match arm (`RuntimeEvent::X { .. } if cond =>`) reads as a
    construction, because the token that follows the payload is `if` and not
    `=>`, so a variant only ever matched looked emitted;
  * the webview dispatches on `category`, not on the variant name, so a variant
    the interface really does act on looked unread whenever its name never
    appears in `ui/src`.

The rule this file holds, therefore:

  emitted   a `RuntimeEvent::X` expression in production Rust, outside the
            desktop bridge.
  consumed  a `RuntimeEvent::X` pattern in production Rust outside the bridge,
            or the variant's name in `ui/src`, or the category the bridge gives
            that variant named in `ui/src`.

The same file also holds the other half of the EventBus contract, the lag rule:
"log at `WARN`, `resubscribe()`, and continue, never panic". It was declared in
two documents and held by nobody, across thirteen subscriber loops that logged
and continued without ever resubscribing. The rule now lives in one place,
`apollia_core::events::ResilientReceiver`, and the check is mechanical: a
`RecvError::Lagged` arm in production Rust anywhere else is a subscriber that
took the rule into its own hands, which is how the thirteen drifted.

A variant emitted and never consumed is wire that informs nobody. A variant
consumed and never emitted is a handler waiting for something that cannot
arrive. A variant neither emitted nor consumed is catalogue that only grows the
contract.

Verdict by exit code, since the caller reads it rather than the text:

  0  every variant is emitted and consumed
  1  at least one variant is orphaned on one side
  2  nothing measured: the enum, the bridge or its `categorize` is absent

Usage:
    python3 scripts/check_eventbus_variants.py [--list] [--json] [--selftest]
"""

import argparse
import contextlib
import io
import json
import os
import re
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

ENUM_FILE = "crates/apollia-core/src/events/runtime_event.rs"
LAG_RULE_FILE = "crates/apollia-core/src/events/resilient.rs"
BRIDGE_FILE = "crates/apollia-desktop/src/events.rs"
UI_ROOT = "crates/apollia-desktop/ui/src"
CRATES_ROOT = "crates"

TOKEN = re.compile(r"RuntimeEvent::([A-Z][A-Za-z0-9]*)\b")
LAG_ARM = re.compile(r"(?<!BroadcastStream)RecvError::Lagged\s*\(")
STREAM_LAG_ARM = re.compile(r"BroadcastStreamRecvError::Lagged\s*\(")

# The SSE routes hand a `BroadcastStream` to axum, and a stream owns its
# receiver: there is no `resubscribe()` to call, so those three sites hold the
# half of the rule that is reachable, naming the drop in a `WARN`. The list is
# named rather than pattern-matched, and a fourth file joining it is a red: an
# exemption nobody counts is an exemption that grows.
STREAM_EXEMPT = (
    "crates/apollia-runtime/src/api/routes_chat.rs",
    "crates/apollia-runtime/src/api/routes_messages.rs",
    "crates/apollia-runtime/src/api/routes_sse.rs",
)
UI_SUFFIXES = (".ts", ".svelte", ".js")


def variants(text: str) -> list[str]:
    """Every variant name declared by the `RuntimeEvent` enum."""
    match = re.search(r"pub enum RuntimeEvent \{(.*?)\n\}", text, re.S)
    if not match:
        return []
    body = re.sub(r"//[^\n]*", "", match.group(1))
    body = re.sub(r"#\[[^\]]*\]", "", body)
    return re.findall(r"\n    ([A-Z][A-Za-z0-9]*)\s*(?:\(|\{|,)", body)


def function_span(text: str, signature: str) -> tuple[int, int] | None:
    """Byte span of the function body that follows `signature`."""
    start = text.find(signature)
    if start < 0:
        return None
    brace = text.find("{", start)
    if brace < 0:
        return None
    depth = 0
    for k in range(brace, len(text)):
        if text[k] == "{":
            depth += 1
        elif text[k] == "}":
            depth -= 1
            if depth == 0:
                return (start, k + 1)
    return None


def categories(bridge_text: str) -> dict[str, str]:
    """Map each variant to the category string `categorize` gives it.

    The webview never sees a variant name on the generic channel; it sees this
    category. Reading the map is what lets a category listener count as a
    reader of the variants that reach it.
    """
    span = function_span(bridge_text, "fn categorize(")
    if span is None:
        return {}
    body = bridge_text[span[0] : span[1]]
    body = re.sub(r"//[^\n]*", "", body)
    result: dict[str, str] = {}
    for arm in re.finditer(r"((?:RuntimeEvent::[^=]*?))=>\s*\{?\s*\"([a-z0-9-]+)\"", body, re.S):
        category = arm.group(2)
        for name in TOKEN.finditer(arm.group(1)):
            result.setdefault(name.group(1), category)
    return result


def skip_payload(text: str, i: int) -> int:
    """Index just past the balanced `( )` or `{ }` group that follows, if any."""
    n = len(text)
    while i < n and text[i] in " \t\r\n":
        i += 1
    if i < n and text[i] in "({":
        depth = 0
        while i < n:
            if text[i] in "({[":
                depth += 1
            elif text[i] in ")}]":
                depth -= 1
                if depth == 0:
                    return i + 1
            i += 1
    return i


def is_pattern(text: str, start: int, end: int) -> bool:
    """True when the `RuntimeEvent::Name` at `start` is matched, not built."""
    j = skip_payload(text, end)
    payload = text[end:j]
    if ".." in payload:
        return True
    n = len(text)
    k = j
    while k < n and text[k] in " \t\r\n)":
        k += 1
    rest = text[k : k + 3]
    if rest.startswith("=>") or rest.startswith("|"):
        return True
    # A guarded arm: `RuntimeEvent::X(v) if v.is_fatal() => ...`. Without this
    # the token after the payload is `if`, and a matched variant reads as built.
    if re.match(r"if[\s(]", rest):
        return True
    if rest[:1] == "=" and not rest.startswith("=="):
        return True
    p = start - 1
    while p >= 0 and text[p] in " \t\r\n":
        p -= 1
    if p >= 0 and text[p] == "|":
        return True
    line_start = text.rfind("\n", 0, start) + 1
    line = text[line_start:start]
    if "matches!(" in line or "if let " in line or "while let " in line:
        return True
    back = text[max(0, start - 200) : start]
    if re.search(r"matches!\(\s*[^,]+,\s*$", back):
        return True
    return False


def blank_comments(text: str) -> str:
    """Blank line comments, keeping every offset and line number."""
    return re.sub(r"//[^\n]*", lambda m: " " * len(m.group(0)), text)


def scan_rust(root: Path, names: list[str]) -> tuple[dict, dict, dict]:
    """Split every production `RuntimeEvent::X` between built and matched.

    The desktop bridge is a pass-through and is counted on neither side: it
    mentions all of the catalogue, both in `categorize` and in its dedicated
    fast paths, so counting it would make every variant look wired.
    """
    emit: dict[str, list[str]] = {v: [] for v in names}
    cons: dict[str, list[str]] = {v: [] for v in names}
    fwd: dict[str, int] = {v: 0 for v in names}
    crates = root / CRATES_ROOT
    bridge = root / BRIDGE_FILE
    for dirpath, _dirs, files in os.walk(crates):
        if "/target" in dirpath or "/node_modules" in dirpath:
            continue
        for filename in files:
            if not filename.endswith(".rs"):
                continue
            path = Path(dirpath) / filename
            text = blank_comments(path.read_text(encoding="utf-8", errors="replace"))
            test_mod = re.search(r"#\[cfg\(test\)\]\s*(pub\s+)?mod\b", text)
            test_offset = test_mod.start() if test_mod else len(text)
            in_tests = (
                "/tests/" in path.as_posix()
                or filename.endswith(("_test.rs", "_tests.rs"))
                or filename == "tests.rs"
            )
            is_bridge = path == bridge
            rel = path.relative_to(root).as_posix()
            for match in TOKEN.finditer(text):
                name = match.group(1)
                if name not in emit:
                    continue
                if in_tests or match.start() >= test_offset:
                    continue
                if is_bridge:
                    fwd[name] += 1
                    continue
                line = text.count("\n", 0, match.start()) + 1
                site = f"{rel}:{line}"
                if is_pattern(text, match.start(), match.end()):
                    cons[name].append(site)
                else:
                    emit[name].append(site)
    return emit, cons, fwd


def scan_ui(root: Path, names: list[str], category_of: dict[str, str]) -> dict[str, list[str]]:
    """Where the webview reads each variant: by its name, or by its category."""
    hits: dict[str, list[str]] = {v: [] for v in names}
    ui_root = root / UI_ROOT
    if not ui_root.is_dir():
        return hits
    wanted_categories = sorted(set(category_of.values()))
    by_category: dict[str, list[str]] = {c: [] for c in wanted_categories}
    name_pattern = re.compile(r"(?<![A-Za-z0-9_])(" + "|".join(names) + r")(?![A-Za-z0-9_])")
    category_pattern = (
        re.compile("|".join(re.escape(f'"{c}"') + "|" + re.escape(f"'{c}'") for c in wanted_categories))
        if wanted_categories
        else None
    )
    for dirpath, _dirs, files in os.walk(ui_root):
        if "/node_modules" in dirpath:
            continue
        for filename in files:
            if not filename.endswith(UI_SUFFIXES) or filename.endswith((".test.ts", ".spec.ts")):
                continue
            path = Path(dirpath) / filename
            text = path.read_text(encoding="utf-8", errors="replace")
            text = re.sub(
                r"/\*.*?\*/|<!--.*?-->",
                lambda m: re.sub(r"[^\n]", " ", m.group(0)),
                text,
                flags=re.S,
            )
            text = re.sub(r"(?<![:\w])//[^\n]*", lambda m: " " * len(m.group(0)), text)
            rel = path.relative_to(root).as_posix()
            for match in name_pattern.finditer(text):
                line = text.count("\n", 0, match.start()) + 1
                hits[match.group(1)].append(f"{rel}:{line}")
            if category_pattern is not None:
                for match in category_pattern.finditer(text):
                    line = text.count("\n", 0, match.start()) + 1
                    by_category[match.group(0).strip("\"'")].append(f"{rel}:{line}")
    for name, category in category_of.items():
        if name in hits:
            hits[name].extend(f"[{category}] {site}" for site in by_category.get(category, [])[:1])
    return hits


def scan_lag_rule(root: Path) -> tuple[list[str], int, int]:
    """Production `Lagged` arms outside the one place that owns the rule.

    Returns (offending sites, arms inside the rule file, exempt stream arms).
    The second number is the positive control: zero of them means the rule moved
    or the pattern stopped matching, and a green verdict would say nothing. The
    third drives the named exemption from its own side, so an exemption that
    grows is visible in a passing run.
    """
    offenders: list[str] = []
    inside = 0
    exempt = 0
    crates = root / CRATES_ROOT
    if not crates.is_dir():
        return offenders, inside
    for dirpath, _dirs, files in os.walk(crates):
        if "/target" in dirpath or "/node_modules" in dirpath:
            continue
        for filename in files:
            if not filename.endswith(".rs"):
                continue
            path = Path(dirpath) / filename
            rel = path.relative_to(root).as_posix()
            text = blank_comments(path.read_text(encoding="utf-8", errors="replace"))
            test_mod = re.search(r"#\[cfg\(test\)\]\s*(pub\s+)?mod\b", text)
            test_offset = test_mod.start() if test_mod else len(text)
            in_tests = (
                "/tests/" in path.as_posix()
                or filename.endswith(("_test.rs", "_tests.rs"))
                or filename == "tests.rs"
            )
            for match in LAG_ARM.finditer(text):
                if in_tests or match.start() >= test_offset:
                    continue
                if rel == LAG_RULE_FILE:
                    inside += 1
                    continue
                line = text.count("\n", 0, match.start()) + 1
                offenders.append(f"{rel}:{line}")
            for match in STREAM_LAG_ARM.finditer(text):
                if in_tests or match.start() >= test_offset:
                    continue
                line = text.count("\n", 0, match.start()) + 1
                names_the_drop = "eventbus.lagged" in text[match.end() : match.end() + 260]
                if rel in STREAM_EXEMPT and names_the_drop:
                    exempt += 1
                    continue
                offenders.append(f"{rel}:{line}")
    return offenders, inside, exempt


def report(root: Path, list_all: bool = False, as_json: bool = False) -> int:
    enum_path = root / ENUM_FILE
    bridge_path = root / BRIDGE_FILE
    if not enum_path.exists():
        print(f"NOTHING MEASURED: {ENUM_FILE} is absent", file=sys.stderr)
        return 2
    names = variants(enum_path.read_text(encoding="utf-8", errors="replace"))
    if not names:
        print("NOTHING MEASURED: the RuntimeEvent enum was not parsed", file=sys.stderr)
        return 2
    if not bridge_path.exists():
        print(f"NOTHING MEASURED: {BRIDGE_FILE} is absent", file=sys.stderr)
        return 2
    category_of = categories(bridge_path.read_text(encoding="utf-8", errors="replace"))
    if not category_of:
        print("NOTHING MEASURED: the bridge's categorize() was not parsed", file=sys.stderr)
        return 2
    uncategorised = [v for v in names if v not in category_of]

    emit, cons, fwd = scan_rust(root, names)
    ui = scan_ui(root, names, category_of)

    never_emitted = [v for v in names if not emit[v]]
    never_consumed = [v for v in names if not cons[v] and not ui[v]]
    dead = [v for v in never_emitted if v in never_consumed]
    emitted_not_consumed = [v for v in never_consumed if v not in never_emitted]
    consumed_not_emitted = [v for v in never_emitted if v not in never_consumed]
    orphans = len(emitted_not_consumed) + len(consumed_not_emitted) + len(dead)

    lag_offenders, lag_inside, lag_exempt = scan_lag_rule(root)
    lag_unmeasured = lag_inside == 0

    payload = {
        "variants": len(names),
        "categories": len(set(category_of.values())),
        "uncategorised": uncategorised,
        "emitted_never_consumed": emitted_not_consumed,
        "consumed_never_emitted": consumed_not_emitted,
        "neither": dead,
        "lag_rule_offenders": lag_offenders,
        "lag_rule_arms_inside": lag_inside,
        "lag_rule_stream_exempt": lag_exempt,
    }
    if as_json:
        payload["emit"] = emit
        payload["consume"] = cons
        payload["ui"] = ui
        payload["category_of"] = category_of
        print(json.dumps(payload, indent=1, sort_keys=True))
    else:
        print(f"RuntimeEvent variants          : {len(names)}")
        print(f"bridge categories              : {len(set(category_of.values()))}")
        print(f"relay mentions (not a reader)  : {sum(fwd.values())}")
        print(f"emitted and consumed           : {len(names) - orphans}")
        print(f"emitted, never consumed        : {len(emitted_not_consumed)}")
        print(f"consumed, never emitted        : {len(consumed_not_emitted)}")
        print(f"neither emitted nor consumed   : {len(dead)}")
        print(f"lag arms inside the rule       : {lag_inside}")
        print(f"lag arms outside the rule      : {len(lag_offenders)}")
        print(f"exempt stream arms (named)     : {lag_exempt} of {len(STREAM_EXEMPT)} file(s)")
        if uncategorised:
            print(f"NOT CATEGORISED BY THE BRIDGE  : {len(uncategorised)} {' '.join(uncategorised)}")
        if lag_offenders:
            print("--- LAG RULE HELD OUTSIDE THE ONE PLACE THAT OWNS IT")
            for site in lag_offenders:
                print(f"  {site}")
        if list_all or orphans:
            for label, group in (
                ("EMITTED-NEVER-CONSUMED", emitted_not_consumed),
                ("CONSUMED-NEVER-EMITTED", consumed_not_emitted),
                ("NEITHER", dead),
            ):
                if not group and not list_all:
                    continue
                print(f"--- {label}")
                for name in group:
                    sites = (emit[name][:2] + cons[name][:2] + ui[name][:1]) or ["-"]
                    print(f"  {name:28s} [{category_of.get(name, '?')}] {' '.join(sites)}")

    stream = sys.stderr if as_json else sys.stdout
    if lag_unmeasured:
        print(
            f"NOTHING MEASURED: no lag arm found in {LAG_RULE_FILE}, so the "
            "lag rule was not checked",
            file=sys.stderr,
        )
        return 2
    bad = orphans or uncategorised or lag_offenders
    verdict = "RED" if bad else "GREEN"
    print(
        f"verdict: {verdict} ({orphans} orphaned variant(s), "
        f"{len(lag_offenders)} lag arm(s) outside the rule)",
        file=stream,
    )
    return 1 if bad else 0


def _case(label: str, ok: bool) -> bool:
    print(f"  {'ok  ' if ok else 'FAIL'}  {label}")
    return ok


def _write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def _subject(tmp: Path, *, orphan: bool) -> Path:
    """A miniature tree with the three relay shapes that fooled the count."""
    root = tmp / "tree"
    enum_variants = ["Built", "Guarded", "ByCategory"] + (["Orphan"] if orphan else [])
    body = "\n".join(f"    {v} {{ id: String }}," for v in enum_variants)
    _write(root / ENUM_FILE, f"pub enum RuntimeEvent {{\n{body}\n}}\n")
    arms = "\n".join(f"        RuntimeEvent::{v} {{ .. }} => \"cat-{v.lower()}\"," for v in enum_variants)
    _write(
        root / BRIDGE_FILE,
        "fn categorize(event: &RuntimeEvent) -> &'static str {\n"
        "    match event {\n"
        f"{arms}\n"
        "    }\n"
        "}\n"
        "fn emit_todo_fastpath(event: &RuntimeEvent) {\n"
        "    if let RuntimeEvent::ByCategory { .. } = event { relay(); }\n"
        "}\n",
    )
    _write(
        root / "crates/apollia-runtime/src/emitters.rs",
        "fn go(bus: &Bus) {\n"
        "    bus.send(RuntimeEvent::Built { id: id.clone() });\n"
        "    bus.send(RuntimeEvent::Guarded { id: id.clone() });\n"
        "    bus.send(RuntimeEvent::ByCategory { id: id.clone() });\n"
        + ("    bus.send(RuntimeEvent::Orphan { id: id.clone() });\n" if orphan else "")
        + "}\n",
    )
    _write(
        root / "crates/apollia-runtime/src/readers.rs",
        "fn read(event: &RuntimeEvent) {\n"
        "    match event {\n"
        "        RuntimeEvent::Built { id } => keep(id),\n"
        "        RuntimeEvent::Guarded { id } if id.is_empty() => keep(id),\n"
        "        _ => {}\n"
        "    }\n"
        "}\n",
    )
    _write(
        root / UI_ROOT / "store.ts",
        'export function dispatch(e: { category: string }) {\n'
        '  if (e.category === "cat-bycategory") refresh();\n'
        "}\n",
    )
    # The lag rule: one arm inside the file that owns it (the positive control),
    # and, when asked for, one taken into a subscriber's own hands.
    _write(
        root / LAG_RULE_FILE,
        "pub async fn recv(&mut self) -> Option<RuntimeEvent> {\n"
        "    match self.receiver.recv().await {\n"
        "        Err(RecvError::Lagged(skipped)) => self.resubscribe(skipped),\n"
        "        other => other.ok(),\n"
        "    }\n"
        "}\n",
    )
    _write(
        root / "crates/apollia-runtime/src/loops.rs",
        "async fn go(rx: &mut Receiver) {\n"
        "    match rx.recv().await {\n"
        + (
            "        Err(RecvError::Lagged(n)) => tracing::warn!(skipped = n, \"lagged\"),\n"
            if orphan
            else ""
        )
        + "        Ok(event) => handle(event),\n"
        "        _ => {}\n"
        "    }\n"
        "}\n"
        "#[cfg(test)]\n"
        "mod tests {\n"
        "    fn t() { let _ = Err(RecvError::Lagged(1)); }\n"
        "}\n",
    )
    return root


def selftest() -> int:
    print("eventbus catalogue: both directions on a built subject")
    results: list[bool] = []
    with tempfile.TemporaryDirectory(prefix="check-eventbus-") as tmp:
        root = _subject(Path(tmp), orphan=True)
        buffer = io.StringIO()
        with contextlib.redirect_stdout(buffer):
            code = report(root, list_all=True)
        text = buffer.getvalue()
        results.append(_case("a variant nobody reads is a red, and it is named", code == 1 and "Orphan" in text))
        results.append(
            _case(
                "a guarded match arm reads as a consumer, not as a construction",
                "Guarded" not in text.split("--- EMITTED-NEVER-CONSUMED")[1].split("---")[0],
            )
        )
        results.append(
            _case(
                "the webview's category listener reads the variants routed to it",
                "ByCategory" not in text.split("--- EMITTED-NEVER-CONSUMED")[1].split("---")[0],
            )
        )
        results.append(
            _case(
                "a subscriber holding the lag rule itself is a red, and it is named",
                "crates/apollia-runtime/src/loops.rs" in text,
            )
        )
        results.append(
            _case(
                "a lag arm under #[cfg(test)] is not a subscriber taking the rule",
                text.count("loops.rs:") == 1,
            )
        )
    with tempfile.TemporaryDirectory(prefix="check-eventbus-") as tmp:
        root = _subject(Path(tmp), orphan=False)
        buffer = io.StringIO()
        with contextlib.redirect_stdout(buffer):
            code = report(root, list_all=True)
        results.append(_case("positive control: the same tree without the orphan is green", code == 0))
        results.append(
            _case(
                "the rule's own arm is the coverage control, not an offence",
                "lag arms inside the rule       : 1" in buffer.getvalue(),
            )
        )
        (root / LAG_RULE_FILE).write_text("pub fn nothing() {}\n", encoding="utf-8")
        probe = io.StringIO()
        with contextlib.redirect_stdout(probe):
            code = report(root, list_all=True)
        results.append(
            _case("a rule file with no lag arm measures nothing", code == 2)
        )
        _subject(Path(tmp), orphan=False)
        # The relay's dedicated fast path mentions ByCategory; on its own it
        # must not save a variant the webview does not read.
        ui = root / UI_ROOT / "store.ts"
        ui.write_text("export function dispatch() { refresh(); }\n", encoding="utf-8")
        buffer = io.StringIO()
        with contextlib.redirect_stdout(buffer):
            code = report(root, list_all=True)
        text = buffer.getvalue()
        results.append(
            _case(
                "the relay's own fast path does not count as a reader",
                code == 1 and "ByCategory" in text,
            )
        )
        (root / ENUM_FILE).write_text("pub struct NotAnEnum;\n", encoding="utf-8")
        buffer = io.StringIO()
        with contextlib.redirect_stdout(buffer):
            code = report(root)
        results.append(_case("an unparsable catalogue measures nothing", code == 2))
    print()
    if all(results):
        print(f"self-test: all {len(results)} cases pass")
        return 0
    print(f"self-test: {results.count(False)} of {len(results)} cases fail")
    return 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--list", action="store_true", help="name every orphaned variant")
    parser.add_argument("--json", action="store_true", help="render the crossing as JSON")
    parser.add_argument(
        "--selftest", action="store_true", help="check the guard itself against a built subject"
    )
    args = parser.parse_args()
    if args.selftest:
        return selftest()
    return report(REPO_ROOT, list_all=args.list, as_json=args.json)


if __name__ == "__main__":
    sys.exit(main())
