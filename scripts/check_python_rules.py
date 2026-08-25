#!/usr/bin/env python3
"""Guard the Python rules of PYTHON-PATTERNS.md, sdk/AGENTS.md and FORBIDDEN.md.

Every rule below is one that no other guard catches: ruff is configured
without the rules that would, or the rule is not expressible in ruff (the
exception root, the agent module contract, the PEP 563 + TypedDict trap).
Each rule is measured by AST where it is syntactic, by source scan where it
is lexical, on the git-tracked Python files of sdk/ and agents/.

A rule can be exempted for one file by a pragma comment naming the rule:

    # REASON: print-call: this module is the SDK CLI; print() is its output.

The exemption is scoped to the file that carries it and to the named rule,
and every exemption used is reported, so a run tells apart "nothing found"
from "found and excused". A measurement that finds nothing is only trusted
because `--selftest` fabricates a subject where every rule fires once, a
clean subject where none does, and an exempted subject where the pragma is
honoured and counted.

Usage:
    check_python_rules.py [--root DIR] [PATH ...]      measure (default: sdk/ agents/)
    check_python_rules.py --selftest                   run the fabricated subjects

Exit codes:
    0  measured, nothing found outside written exemptions
    1  measured, at least one rule fires
    2  nothing measured (no Python file under the given paths)
    3  self-test failure
"""

import argparse
import ast
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

# Python 3.12 stdlib: the running interpreter's list, plus the modules 3.13
# removed (PEP 594) so that a 3.13 host still judges against the 3.12 floor.
_DEAD_BATTERIES = {
    "aifc", "audioop", "cgi", "cgitb", "chunk", "crypt", "imghdr", "lib2to3",
    "mailcap", "msilib", "nis", "nntplib", "ossaudiodev", "pipes", "sndhdr",
    "spwd", "sunau", "telnetlib", "uu", "xdrlib",
}
STDLIB = set(sys.stdlib_module_names) | _DEAD_BATTERIES

# First-party roots: the SDK package, its test package, and the agents'
# own packages (an agent imports itself absolutely, per PYTHON-PATTERNS §5).
FIRST_PARTY = {"apollia", "tests"}
# Declared in sdk/pyproject.toml [project.optional-dependencies] dev: allowed in test modules only.
DEV_DEPS = {"pytest", "pytest_asyncio", "pytest_cov", "mypy"}

BUILTIN_EXC = {
    "Exception", "BaseException", "RuntimeError", "ValueError", "TypeError",
    "KeyError", "OSError", "IOError", "LookupError", "AttributeError",
}
# The root of the hierarchy is the one class allowed to subclass Exception.
EXC_ROOT = "AgentError"

BARE_TODO_RE = re.compile(r"\b(TODO|FIXME|XXX|HACK)\b(?!\()")
INTERNAL_REF_RE = re.compile(
    r"\b(ADR-\d+|LOT-\d+|CAP-\d+|GRP-\d+|[Ss]print\s+\d+|[Ss]tory\s+\d+)\b|docs/internal"
)
EM_DASH = chr(0x2014)  # the character is never written literally here: this file must pass its own rule
FRENCH_ACCENT_RE = re.compile(r"[éèêàçùâîôûœÉÈÊÀÇ]")
FRENCH_WORD_RE = re.compile(
    r"\b(le|la|les|des|une|est|pour|avec|dans|sur|pas|nous|vous|sont|cette|qui|que|"
    r"mais|donc|aussi|être|sans|selon|chaque|toujours|jamais)\b"
)
# A double-quoted span inside a comment or docstring line is a citation (a
# user's own words, an example string), not prose written in French.
QUOTED_SPAN_RE = re.compile(r'"[^"]*"')
REASON_PRAGMA_RE = re.compile(r"#\s*REASON:\s*([a-z][a-z0-9-]*):")

RULES = [
    ("relative-import", "relative import (PYTHON-PATTERNS §5, FORBIDDEN Python)"),
    ("print-call", "print() outside tests (PYTHON-PATTERNS §6, FORBIDDEN Python)"),
    ("future-typeddict", "from __future__ import annotations in a TypedDict module (FORBIDDEN Python)"),
    ("third-party-import", "import outside stdlib 3.12 and first party (FORBIDDEN Python)"),
    ("exception-direct", f"exception class not rooted at {EXC_ROOT} (PYTHON-PATTERNS §4)"),
    ("typing-star", "from typing import * (PYTHON-PATTERNS §5)"),
    ("no-module-docstring", "module without docstring (ruff D100 is ignored in sdk/pyproject.toml)"),
    ("bare-todo", "bare TODO/FIXME/XXX/HACK (FORBIDDEN prose)"),
    ("french-comment", "French in a comment or docstring (FORBIDDEN prose, inline doc English only)"),
    ("internal-ref", "internal tracker reference (FORBIDDEN prose)"),
    ("em-dash", "em-dash (FORBIDDEN prose)"),
    ("agent-shape", "agent module: @agent class with at least one @skill/@on_message/@orchestrated"),
    ("agent-handwritten", "agent module: hand-written module-level `agent = ...` (PYTHON-PATTERNS §2)"),
]


def _is_test_path(path: Path) -> bool:
    parts = path.parts
    return "tests" in parts or path.name.startswith("test_") or path.name.startswith("conftest")


def _is_agent_module(path: Path) -> bool:
    """An agent module is an `agent.py` shipped next to its `manifest.toml`."""
    return path.name == "agent.py" and (path.parent / "manifest.toml").exists()


def _decorator_name(node: ast.expr) -> str:
    if isinstance(node, ast.Call):
        node = node.func
    if isinstance(node, ast.Attribute):
        return node.attr
    if isinstance(node, ast.Name):
        return node.id
    return ""


def _file_exemptions(source: str) -> set[str]:
    """Rules the file exempts for itself with `# REASON: <rule>:` pragmas."""
    out: set[str] = set()
    for line in source.splitlines():
        m = REASON_PRAGMA_RE.search(line)
        if m:
            out.add(m.group(1))
    return out


def _comment_and_docstring_lines(tree: ast.Module, source: str) -> list[tuple[int, str]]:
    """Return (lineno, text) for every comment line and docstring line."""
    out: list[tuple[int, str]] = []
    for i, line in enumerate(source.splitlines(), 1):
        stripped = line.strip()
        if stripped.startswith("#"):
            out.append((i, stripped))
    for node in ast.walk(tree):
        if isinstance(node, (ast.Module, ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)):
            doc_node = node.body[0] if node.body else None
            if (
                isinstance(doc_node, ast.Expr)
                and isinstance(doc_node.value, ast.Constant)
                and isinstance(doc_node.value.value, str)
            ):
                start = doc_node.lineno
                for k, dl in enumerate(doc_node.value.value.splitlines()):
                    out.append((start + k, dl))
    return out


def _looks_french(text: str) -> bool:
    # A quoted span is a citation, not prose: strip it before judging, so an
    # English sentence quoting a user's French words does not fire.
    text = QUOTED_SPAN_RE.sub('""', text)
    if FRENCH_ACCENT_RE.search(text):
        return True
    return len(set(m.lower() for m in FRENCH_WORD_RE.findall(text))) >= 3


def measure_file(
    path: Path, root: Path, first_party: set[str]
) -> tuple[dict[str, list[str]], dict[str, int]]:
    rel = path.relative_to(root).as_posix()
    hits: dict[str, list[str]] = {rule: [] for rule, _ in RULES}
    exempted: dict[str, int] = {}
    source = path.read_text(encoding="utf-8")
    try:
        tree = ast.parse(source, filename=str(path))
    except SyntaxError as exc:
        hits.setdefault("syntax-error", []).append(f"{rel}:{exc.lineno}: {exc.msg}")
        return hits, exempted

    exemptions = _file_exemptions(source)

    def record(rule: str, site: str) -> None:
        if rule in exemptions:
            exempted[rule] = exempted.get(rule, 0) + 1
        else:
            hits[rule].append(site)

    is_test = _is_test_path(path)
    has_future = any(
        isinstance(n, ast.ImportFrom) and n.module == "__future__"
        and any(a.name == "annotations" for a in n.names)
        for n in tree.body
    )
    defines_typeddict = False
    decorated_agent_class = None
    skill_like = 0
    handwritten_agent = []

    for node in ast.walk(tree):
        if isinstance(node, ast.ImportFrom):
            if node.level and node.level > 0:
                record("relative-import", f"{rel}:{node.lineno}: from {'.' * node.level}{node.module or ''} import ...")
            if node.module == "typing" and any(a.name == "*" for a in node.names):
                record("typing-star", f"{rel}:{node.lineno}: from typing import *")
            if node.level == 0 and node.module:
                top = node.module.split(".")[0]
                if top not in STDLIB and top not in first_party and top != "__future__" and not (is_test and top in DEV_DEPS):
                    record("third-party-import", f"{rel}:{node.lineno}: from {node.module} import ...")
        elif isinstance(node, ast.Import):
            for alias in node.names:
                top = alias.name.split(".")[0]
                if top not in STDLIB and top not in first_party and not (is_test and top in DEV_DEPS):
                    record("third-party-import", f"{rel}:{node.lineno}: import {alias.name}")
        elif isinstance(node, ast.Call):
            if isinstance(node.func, ast.Name) and node.func.id == "print" and not is_test:
                record("print-call", f"{rel}:{node.lineno}: print(...)")
        elif isinstance(node, ast.ClassDef):
            base_names = [_decorator_name(b) for b in node.bases]
            if "TypedDict" in base_names:
                defines_typeddict = True
            if node.name != EXC_ROOT and any(b in BUILTIN_EXC for b in base_names):
                record("exception-direct", f"{rel}:{node.lineno}: class {node.name}({', '.join(base_names)})")
            if any(_decorator_name(d) == "agent" for d in node.decorator_list):
                decorated_agent_class = node.name
                for item in node.body:
                    if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
                        if any(_decorator_name(d) in {"skill", "on_message", "orchestrated"} for d in item.decorator_list):
                            skill_like += 1

    for node in tree.body:
        if isinstance(node, (ast.Assign, ast.AnnAssign)):
            targets = node.targets if isinstance(node, ast.Assign) else [node.target]
            if any(isinstance(t, ast.Name) and t.id == "agent" for t in targets):
                handwritten_agent.append(f"{rel}:{node.lineno}: agent = ...")

    if has_future and defines_typeddict:
        record("future-typeddict", f"{rel}: from __future__ import annotations + TypedDict class")
    if ast.get_docstring(tree) is None:
        record("no-module-docstring", f"{rel}: no module docstring")

    for lineno, text in _comment_and_docstring_lines(tree, source):
        if BARE_TODO_RE.search(text):
            record("bare-todo", f"{rel}:{lineno}: {text[:90]}")
        if _looks_french(text):
            record("french-comment", f"{rel}:{lineno}: {text[:90]}")
    for i, line in enumerate(source.splitlines(), 1):
        if INTERNAL_REF_RE.search(line):
            record("internal-ref", f"{rel}:{i}: {line.strip()[:90]}")
        if EM_DASH in line:
            record("em-dash", f"{rel}:{i}: {line.strip()[:90]}")

    if _is_agent_module(path):
        if decorated_agent_class is None:
            record("agent-shape", f"{rel}: no @agent class")
        elif skill_like == 0:
            record("agent-shape", f"{rel}: @agent class {decorated_agent_class} has no @skill/@on_message/@orchestrated")
        for site in handwritten_agent:
            record("agent-handwritten", site)
    return hits, exempted


def tracked_python(root: Path, paths: list[str]) -> list[Path]:
    """Tracked .py files under the given paths (git ls-files), so ignored venvs and build dirs stay out."""
    try:
        out = subprocess.run(
            ["git", "-C", str(root), "ls-files", "--", *[f"{p}" for p in paths]],
            check=True, capture_output=True, text=True,
        ).stdout
        files = [root / line for line in out.splitlines() if line.endswith(".py")]
        if files:
            return files
    except (subprocess.CalledProcessError, FileNotFoundError):
        pass
    files = []
    for p in paths:
        base = root / p
        if base.is_file() and base.suffix == ".py":
            files.append(base)
        elif base.is_dir():
            files.extend(f for f in base.rglob("*.py") if ".venv" not in f.parts and "build" not in f.parts)
    return files


def run(root: Path, paths: list[str], quiet: bool = False) -> tuple[int, dict[str, list[str]], dict[str, int]]:
    files = tracked_python(root, paths)
    if not files:
        if not quiet:
            print(f"nothing measured: no Python file under {paths}")
        return 2, {}, {}
    first_party = set(FIRST_PARTY)
    for f in files:
        if f.name == "agent.py":
            first_party.add(f.parent.name.replace("-", "_"))
            first_party.add(f.parent.name)
    totals: dict[str, list[str]] = {rule: [] for rule, _ in RULES}
    exempted: dict[str, int] = {}
    for f in sorted(files):
        file_hits, file_exempted = measure_file(f, root, first_party)
        for rule, sites in file_hits.items():
            totals.setdefault(rule, []).extend(sites)
        for rule, n in file_exempted.items():
            exempted[rule] = exempted.get(rule, 0) + n
    if not quiet:
        print(f"files measured: {len(files)} under {', '.join(paths)}")
        for rule, label in RULES:
            n = len(totals[rule])
            print(f"  {rule:22s} {n:4d}   {label}")
            for site in totals[rule]:
                print(f"      {site}")
        extra = {k: v for k, v in totals.items() if k not in dict(RULES)}
        for k, v in extra.items():
            print(f"  {k:22s} {len(v):4d}")
            for site in v:
                print(f"      {site}")
        for rule, n in sorted(exempted.items()):
            print(f"  exempted by # REASON: {rule}: {n} site(s)")
    fired = sum(len(v) for v in totals.values())
    return (1 if fired else 0), totals, exempted


# The tracker identifier is assembled at runtime so this guard passes the
# tree-level prose rule its own fixture exists to exercise.
DIRTY_AGENT = '''\
from __future__ import annotations
from .sibling import thing
from typing import *
import requests
from typing import TypedDict
# TODO: fix later (see CAP''' + '''-042)
# ceci est un commentaire écrit en français
class P(TypedDict):
    a: int
class MyErr(Exception):
    pass
def f():
    print("x")  # em dash here: ''' + chr(0x2014) + '''
class Foo:
    pass
agent = Foo()
'''

CLEAN_AGENT = '''\
"""A clean agent module.

The user said "toujours local" and we keep that citation verbatim here to
pin that a quoted span is not judged as French prose.
"""

import json

from apollia import agent, on_message
from apollia.types import Ctx


@agent(name="clean", version="0.1.0", description="clean")
class Clean:
    @on_message
    async def handle(self, ctx: Ctx, message: str) -> str:
        return json.dumps({"ok": message})
'''

EXEMPTED_MODULE = '''\
"""A CLI-like module whose print() calls carry a written exemption."""

# REASON: print-call: this module is a CLI entry point; print() is its
# user-facing output channel, the same carve-out the Rust rule gives
# apollia-cli.


def emit() -> None:
    print("one")
    print("two")
'''


def selftest() -> int:
    expected_dirty = {
        "relative-import": 1, "print-call": 1, "future-typeddict": 1, "third-party-import": 1,
        "exception-direct": 1, "typing-star": 1, "no-module-docstring": 1, "bare-todo": 1,
        "french-comment": 1, "internal-ref": 1, "em-dash": 1, "agent-shape": 1, "agent-handwritten": 1,
    }
    failures = 0
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        (root / "dirty").mkdir()
        (root / "dirty" / "agent.py").write_text(DIRTY_AGENT, encoding="utf-8")
        (root / "dirty" / "manifest.toml").write_text('name = "dirty"\n', encoding="utf-8")
        (root / "clean").mkdir()
        (root / "clean" / "agent.py").write_text(CLEAN_AGENT, encoding="utf-8")
        (root / "clean" / "manifest.toml").write_text('name = "clean"\n', encoding="utf-8")
        (root / "excused").mkdir()
        (root / "excused" / "cli.py").write_text(EXEMPTED_MODULE, encoding="utf-8")
        code, totals, _ = run(root, ["dirty"], quiet=True)
        print("self-test: fabricated dirty subject")
        for rule, want in expected_dirty.items():
            got = len(totals.get(rule, []))
            ok = got == want
            failures += 0 if ok else 1
            print(f"  {'ok ' if ok else 'KO '} {rule:22s} expected {want}, got {got}")
        ok = code == 1
        failures += 0 if ok else 1
        print(f"  {'ok ' if ok else 'KO '} exit code on dirty subject: expected 1, got {code}")
        code, totals, _ = run(root, ["clean"], quiet=True)
        fired = {k: v for k, v in totals.items() if v}
        ok = code == 0 and not fired
        failures += 0 if ok else 1
        print(f"  {'ok ' if ok else 'KO '} positive control: clean subject (with a quoted French citation) fires nothing (exit {code}, fired {sorted(fired)})")
        code, totals, exempted = run(root, ["excused"], quiet=True)
        ok = code == 0 and not totals.get("print-call") and exempted.get("print-call") == 2
        failures += 0 if ok else 1
        print(f"  {'ok ' if ok else 'KO '} a # REASON: print-call: pragma excuses the file and is counted (exit {code}, exempted {exempted})")
        unpragma = EXEMPTED_MODULE.replace("REASON: print-call:", "REASON: some-other-rule:")
        (root / "excused" / "cli.py").write_text(unpragma, encoding="utf-8")
        code, totals, exempted = run(root, ["excused"], quiet=True)
        ok = code == 1 and len(totals.get("print-call", [])) == 2 and not exempted
        failures += 0 if ok else 1
        print(f"  {'ok ' if ok else 'KO '} a pragma naming another rule excuses nothing (exit {code})")
        code, _, _ = run(root, ["absent"], quiet=True)
        ok = code == 2
        failures += 0 if ok else 1
        print(f"  {'ok ' if ok else 'KO '} empty subject renders exit 2, got {code}")
    print(f"self-test: {'all cases pass' if not failures else f'{failures} case(s) fail'}")
    return 0 if not failures else 3


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--root", default=str(REPO_ROOT), help="repository root (default: the tree this script lives in)")
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("paths", nargs="*", default=["sdk", "agents"])
    args = ap.parse_args()
    if args.selftest:
        return selftest()
    code, _, _ = run(Path(args.root).resolve(), args.paths)
    return code


if __name__ == "__main__":
    sys.exit(main())
