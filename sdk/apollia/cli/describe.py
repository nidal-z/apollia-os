"""``apollia describe`` - write the tool and workflow descriptors of a package.

The command imports every Python module of a package folder, collects the
functions marked by ``@tool`` and ``@workflow``, and rewrites the ``[[tools]]``
and ``[[workflows]]`` tables of the folder's ``apollia.toml`` from them: name,
module, description, action family, input schema, and the steps of a
workflow. Everything else in the manifest is kept as written. Whoever reads
the manifest afterwards learns what the package ships without running it.

Sensitivity is not written: ``apollia.toml`` derives it from the family.
"""

# REASON: print-call: this module is the SDK's command-line surface and has
# no ctx; print() is its user-facing output channel, the same carve-out the
# Rust rule grants apollia-cli. Read by scripts/check_python_rules.py.

from __future__ import annotations

import importlib
import json
import math
import os
import re
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

from apollia.errors import AgentConfigError, AgentError
from apollia.tool_decorator import TOOL_ATTR, ToolSpec
from apollia.workflow_decorator import WORKFLOW_ATTR, WorkflowSpec

if TYPE_CHECKING:
    import argparse
    from types import ModuleType

__all__ = ["build_parser", "describe_package", "render_tables", "rewrite_manifest"]

MANIFEST_NAME = "apollia.toml"

#: The comment that opens the tables this command writes.
GENERATED_MARKER = (
    "# Written by `python -m apollia describe` from the code; edits here are replaced."
)

# Folders never imported: tests, virtual environments, caches.
_SKIPPED_DIRS = frozenset({"tests", "test", "venv", "node_modules", "__pycache__"})

_BARE_KEY = re.compile(r"^[A-Za-z0-9_-]+$")
_HEADER = re.compile(r"^\s*\[\[?\s*([A-Za-z0-9_-]+)(?:\s*\.\s*[A-Za-z0-9_-]+)*\s*\]\]?\s*(?:#.*)?$")
_OWNED_TABLES = frozenset({"tools", "workflows"})


@dataclass(frozen=True)
class Described:
    """One declaration found in the package, with the module that holds it."""

    module: str
    spec: ToolSpec | WorkflowSpec


# ──────────────────────────────────────────────────────────────────────
# Discovery
# ──────────────────────────────────────────────────────────────────────


def _python_files(root: Path) -> list[Path]:
    files: list[Path] = []
    for path in sorted(root.rglob("*.py")):
        parts = path.relative_to(root).parts
        if any(p.startswith(".") or p in _SKIPPED_DIRS for p in parts[:-1]):
            continue
        if parts[-1].startswith("test_"):
            continue
        files.append(path)
    return files


def _dotted(root: Path, path: Path) -> str:
    parts = list(path.relative_to(root).with_suffix("").parts)
    if parts[-1] == "__init__":
        parts.pop()
    return ".".join(parts)


def _lives_under(module: ModuleType, root: Path) -> bool:
    """``True`` when ``module`` was loaded from ``root``, namespace packages included."""
    file = getattr(module, "__file__", None)
    places = [file] if file else list(getattr(module, "__path__", []))
    return bool(places) and all(Path(p).resolve().is_relative_to(root) for p in places)


def _import_all(root: Path, files: list[Path]) -> list[tuple[Path, ModuleType]]:
    """Import each file under its dotted name with ``root`` on ``sys.path``."""
    loaded: list[tuple[Path, ModuleType]] = []
    for path in files:
        dotted = _dotted(root, path)
        if not dotted:
            continue
        top = dotted.split(".")[0]
        already = sys.modules.get(top)
        if already is not None and not _lives_under(already, root):
            raise AgentConfigError(
                f"{path.relative_to(root).as_posix()}: the module name {top!r} is already "
                "taken by an imported module; rename the file or its folder"
            )
        loaded.append((path, importlib.import_module(dotted)))
    return loaded


def describe_package(root: Path) -> list[Described]:
    """Every ``@tool`` and ``@workflow`` of the package at ``root``, in file order.

    Raises:
        AgentConfigError: when two declarations share a name, or a module name
            collides with a module already imported.
        Exception: whatever importing a module of the package raises.
    """
    root = root.resolve()
    before = set(sys.modules)
    sys.path.insert(0, str(root))
    try:
        found: list[Described] = []
        for path, module in _import_all(root, _python_files(root)):
            relative = path.relative_to(root).as_posix()
            for value in list(vars(module).values()):
                if getattr(value, "__module__", None) != module.__name__:
                    continue
                spec = getattr(value, TOOL_ATTR, None) or getattr(value, WORKFLOW_ATTR, None)
                if isinstance(spec, (ToolSpec, WorkflowSpec)):
                    found.append(Described(relative, spec))
    finally:
        sys.path.remove(str(root))
        for name in set(sys.modules) - before:
            if _lives_under(sys.modules[name], root):
                sys.modules.pop(name, None)
    seen: dict[str, str] = {}
    for item in found:
        if item.spec.name in seen:
            raise AgentConfigError(
                f"{item.spec.name!r} is declared twice, in {seen[item.spec.name]} "
                f"and in {item.module}"
            )
        seen[item.spec.name] = item.module
    return found


# ──────────────────────────────────────────────────────────────────────
# TOML writing
# ──────────────────────────────────────────────────────────────────────


def _key(key: str) -> str:
    return key if _BARE_KEY.match(key) else json.dumps(key, ensure_ascii=False)


def toml_value(value: object) -> str:
    """Render ``value`` as an inline TOML value.

    Raises:
        AgentConfigError: on ``None`` or a non-finite float, which TOML
            cannot carry, or on a type it does not know.
    """
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        if not math.isfinite(value):
            raise AgentConfigError(f"{value!r} cannot be written to TOML")
        return repr(value)
    if isinstance(value, str):
        # A JSON string is a valid TOML basic string: same quotes, same escapes.
        return json.dumps(value, ensure_ascii=False)
    if isinstance(value, (list, tuple)):
        return "[" + ", ".join(toml_value(v) for v in value) + "]"
    if isinstance(value, dict):
        if not value:
            return "{}"
        inner = ", ".join(f"{_key(str(k))} = {toml_value(v)}" for k, v in value.items())
        return "{ " + inner + " }"
    raise AgentConfigError(f"a {type(value).__name__} cannot be written to TOML")


def render_tables(items: list[Described]) -> str:
    """The ``[[tools]]`` then ``[[workflows]]`` tables for ``items``."""
    blocks: list[str] = []
    for item in items:
        if isinstance(item.spec, ToolSpec):
            blocks.append(_render(item, "tools", None))
    for item in items:
        if isinstance(item.spec, WorkflowSpec):
            blocks.append(_render(item, "workflows", list(item.spec.steps)))
    return "\n".join(blocks)


def _render(item: Described, table: str, steps: list[str] | None) -> str:
    spec = item.spec
    lines = [
        f"[[{table}]]",
        f"name = {toml_value(spec.name)}",
        f"module = {toml_value(item.module)}",
        f"description = {toml_value(spec.description)}",
        f"action = {toml_value(spec.action)}",
        f"inputs = {toml_value(spec.input_schema)}",
    ]
    if steps is not None:
        lines.append(f"steps = {toml_value(steps)}")
    return "\n".join(lines) + "\n"


def _owned(line: str) -> bool | None:
    """Classify a line as a header of an owned table, another header, or neither.

    Returns ``True`` for a header of a table this command owns, ``False`` for
    another header, ``None`` for a line that is not a header.
    """
    match = _HEADER.match(line)
    if match is None:
        return None
    return match.group(1) in _OWNED_TABLES


def rewrite_manifest(text: str, tables: str) -> str:
    """Replace the ``[[tools]]`` and ``[[workflows]]`` tables of ``text``.

    Every other line is kept as written. A comment or blank line that closes
    a removed table and precedes the next header is kept, since it introduces
    that header.
    """
    kept: list[str] = []
    held: list[str] = []
    inside = False
    for line in text.splitlines():
        if line.strip() == GENERATED_MARKER:
            continue
        owned = _owned(line)
        if owned is not None:
            if inside and not owned:
                kept.extend(held)
            held = []
            inside = owned
            if owned:
                continue
        if inside:
            stripped = line.strip()
            if not stripped or stripped.startswith("#"):
                held.append(line)
            else:
                held = []
            continue
        kept.append(line)
    while kept and not kept[-1].strip():
        kept.pop()
    head = "\n".join(kept)
    if not tables:
        return head + "\n"
    return f"{head}\n\n{GENERATED_MARKER}\n{tables}"


def _check_round_trip(before: str, after: str, tables: str) -> None:
    """On Python 3.11+, prove the rewrite touched the two tables only."""
    try:
        import tomllib
    except ImportError:  # Python 3.10: no reader in the standard library.
        return
    try:
        old = tomllib.loads(before)
        new = tomllib.loads(after)
        written = tomllib.loads(tables) if tables else {}
    except tomllib.TOMLDecodeError as exc:
        raise AgentConfigError(f"{MANIFEST_NAME} would not be valid TOML: {exc}") from exc
    for table in _OWNED_TABLES:
        old.pop(table, None)
        if new.pop(table, None) != written.get(table):
            raise AgentConfigError(f"the [[{table}]] tables were not rewritten as generated")
    if old != new:
        raise AgentConfigError(f"rewriting {MANIFEST_NAME} would change other tables")


def describe_command(folder: Path) -> int:
    """Describe the package at ``folder`` into its ``apollia.toml``."""
    manifest = folder / MANIFEST_NAME
    if not manifest.is_file():
        print(f"error: {manifest} not found; write its [package] table first", file=sys.stderr)
        return 1
    try:
        items = describe_package(folder)
        before = manifest.read_text(encoding="utf-8")
        tables = render_tables(items)
        after = rewrite_manifest(before, tables)
        _check_round_trip(before, after, tables)
    except AgentError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    except Exception as exc:
        print(f"error: importing the package failed: {type(exc).__name__}: {exc}", file=sys.stderr)
        return 1
    descriptor, temporary = tempfile.mkstemp(dir=folder, prefix=".apollia.toml.")
    with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
        handle.write(after)
    Path(temporary).replace(manifest)
    tools = sum(isinstance(i.spec, ToolSpec) for i in items)
    workflows = len(items) - tools
    print(f"{manifest}: {tools} tool(s), {workflows} workflow(s) described")
    return 0


def build_parser(subparsers: argparse._SubParsersAction[argparse.ArgumentParser]) -> None:
    """Register ``describe`` on the top-level sub-parser group."""
    parser = subparsers.add_parser(
        "describe",
        help="Write the [[tools]] and [[workflows]] of a package's apollia.toml",
    )
    parser.add_argument("folder", help="Package folder holding apollia.toml")

    def _run(ns: argparse.Namespace) -> int:
        return describe_command(Path(ns.folder))

    parser.set_defaults(func=_run)
