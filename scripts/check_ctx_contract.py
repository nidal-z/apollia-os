#!/usr/bin/env python3
"""Cross the two halves of the `ctx` contract: the published protocol and the bridge.

`ctx` is documented on one side and executed on the other. The published side is
the set of `Protocol` classes under `sdk/apollia/context/`, which nothing ever
runs; the executed side is the `#[pyclass(name = "RuntimeContext")]` of
`crates/apollia-aip/src/context.rs` and the classes its accessors hand back.
Twenty-two divergences lived between the two at once, over forty-two reported
lines, and every one of them was published as fact by
`docs/site/scripts/gen_sdk_ref.py`, which reads the published side alone.

The two halves do not replace one another. They split authority fact by fact:

  * the protocol is authoritative on prose, docstrings and named types, because
    the Rust doc-comment is never published and the Python-visible type is only
    knowable there;
  * the bridge is authoritative on whether a member exists, on the name and
    rank of its parameters, on which of them are keyword-only, on whether a
    parameter carries a default, and on whether the accessor can hand back
    `None`, because `#[pyo3(signature = ...)]` is what CPython actually binds.

This module is the only reader of the bridge side in the tree, and it has two
consumers: this guard, and `docs/site/scripts/gen_sdk_ref.py`, which imports
`cross()` and refuses to write a page when the junction carries a divergence. A
second reader would drift from this one at the first `#[cfg]`, and the pages
could then be green while the guard is red.

Nullability is crossed in one direction only, and the missing one is named
rather than left silent: an annotation carrying `| None` demands a service the
bridge really leaves unattached, because publishing an optionality the bridge
cannot produce makes the page wrong. The reverse, an accessor the bridge leaves
unattached under an annotation that promises a value, is *not* red here. It
holds on six services today, and turning it red would be a public contract
decision rather than a correction, so it is tracked outside this guard.

A null branch in an accessor is not the same fact as a service an agent can
find absent, and reading the first as the second published a falsehood on seven
pages: `ctx.a2a`, `ctx.budget`, `ctx.datasources`, `ctx.events`, `ctx.mail`,
`ctx.secrets` and `ctx.templates` all carry a `None =>` arm that no production
path reaches, and the bridge says so in the same file. So the verdict is not
inferred from the syntax, it is read from the accessor's doc comment, where the
bridge writes `ctx-attachment: optional` or `ctx-attachment: always`. An
accessor that can hand back `None` without a verdict is red, because nothing
then says whether the branch is reachable; an `optional` verdict on an accessor
with no such branch is red too, because it claims an absence the code cannot
produce.

What is compared, member by member: the name, then the parameter list as Python
sees it, rank by rank, each with its name, its keyword-only status and the
presence of a default. Not the types: mapping `str | None` onto `Option<String>`
needs a correspondence table that would become a source of false positives, and
the whole documentation campaign turned up exactly one type divergence over
these seventeen pages. Not the value of a default either, except when one side
has none.

Verdict by exit code, since the caller reads it rather than the text:

  0  every service, class and member is either paired or set aside by a named
     rule, and the crossing found no divergence
  1  at least one divergence, or one member, class or service left
     uninterpreted, or a declared exception that no longer has an object
  2  nothing was measured, so the run says nothing about the tree: one of the
     two subtrees is absent, no protocol class was read, no `#[pymethods]`
     block was found, or fewer than fifteen services were paired

The floor of fifteen paired services is the point of the 2. A guard that reads
one service and finds it in agreement would otherwise print "no divergence"
having verified nothing, and "no divergence" would read the same as "no
reading".

Every member on both sides falls into exactly one bucket: paired, or set aside
by a rule named below. A member that falls into neither is left uninterpreted
and the verdict is 1, because an empty divergence list is also what a guard that
stopped reading one of its two sides would print.

Run it from anywhere; the two subtrees are resolved from this file's location.
Run it with `--selftest` to check the guard itself against a built subject.
"""

import argparse
import ast
import contextlib
import io
import re
import shutil
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SDK_SUBTREE = Path("sdk/apollia")
BRIDGE_SUBTREE = Path("crates/apollia-aip/src")

# ── declared exceptions ──────────────────────────────────────────────────────
#
# Each entry is a written verdict, not a silencer. An entry whose subject has
# disappeared makes the guard fail, so the list cannot outlive what it excuses.

CTX_MEMBERS_THE_PROTOCOL_DOES_NOT_PUBLISH = {
    "agent_name": "the name of the running agent, a bridge-side identity the "
    "protocol has never carried",
    "user_context": "user memory injected in chat mode, read by the built-in "
    "assistant and by nothing an agent author writes",
    "user_memory_writable": "a permission flag the runtime sets from the "
    "manifest, not a service",
    "log": "the low-level tracing hook; agents are told to use ctx.logger",
    "step_budget": "the deprecated predecessor of ctx.budget, kept callable "
    "and deliberately unpublished",
}

SERVICES_WITH_NO_BRIDGE_CLASS = {
    "logger": "an alias onto logging.Logger, built by the accessor at call "
    "time; there is no #[pymethods] block to cross",
}

# `RuntimeContext` is not listed here: it is the root of the crossing, paired at
# the service level, and skipping it is structural rather than a written verdict.
BRIDGE_CLASSES_OUTSIDE_THE_CROSSING = {
    "TokenStream": "carries only __aiter__ and __anext__, both special methods",
    "StepBudgetView": "reachable only through ctx.step_budget, itself an "
    "unpublished bridge member",
}

PROTOCOL_CLASSES_OUTSIDE_THE_CROSSING = {
    "SkillCard": "no bridge counterpart: the bridge hands back plain Python "
    "dicts, so there is nothing to cross",
    "MailMessage": "no bridge counterpart, same reason",
    "ToolDescriptor": "no bridge counterpart, same reason",
    "TokenUsage": "a bridge counterpart exists but exposes its fields through "
    "#[pyo3(get)] rather than #[pymethods]",
    "LlmResponse": "same as TokenUsage",
}

SPECIAL_BRIDGE_MEMBERS = ("__repr__", "__aiter__", "__anext__", "__str__")

# The floor of paired services below which the run has measured nothing.
SERVICE_FLOOR = 15


@dataclass(frozen=True)
class Declared:
    """The written exceptions a crossing runs against, and its service floor.

    They travel together because they describe one subject. The self-test runs
    on a fabricated subject that carries none of them, and passing an empty
    record is how it says so, rather than by muting the staleness check.
    """

    ctx_members_the_protocol_does_not_publish: dict
    services_with_no_bridge_class: dict
    bridge_classes_outside_the_crossing: dict
    protocol_classes_outside_the_crossing: dict
    service_floor: int


REAL_TREE = Declared(
    CTX_MEMBERS_THE_PROTOCOL_DOES_NOT_PUBLISH,
    SERVICES_WITH_NO_BRIDGE_CLASS,
    BRIDGE_CLASSES_OUTSIDE_THE_CROSSING,
    PROTOCOL_CLASSES_OUTSIDE_THE_CROSSING,
    SERVICE_FLOOR,
)

FABRICATED_SUBJECT = Declared({}, {}, {}, {}, 1)

# ── Rust reading ─────────────────────────────────────────────────────────────

PYMETHODS_BLOCK = re.compile(r"#\[pymethods\]\s*\nimpl\s+(?:<[^>]*>\s*)?([A-Za-z_]\w*)")
PYCLASS_ATTR = re.compile(r"#\[pyclass(?:\(([^\)]*)\))?\]\s*\n(?:pub\s+)?struct\s+(\w+)")
PYCLASS_NAME = re.compile(r'name\s*=\s*"([^"]+)"')
SIGNATURE_ATTR = re.compile(r"#\[pyo3\(\s*signature\s*=\s*\(")
FN_HEAD = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)")
SELF_FIELD = re.compile(r"self\.(\w+)")
# The attachment verdict the bridge writes in an accessor's doc comment.
ATTACHMENT = re.compile(r"^\s*///\s*ctx-attachment:\s*(\w+)", re.M)
ATTACHMENT_VERDICTS = ("always", "optional")
STRUCT_BLOCK = re.compile(r"(?:pub\s+)?struct\s+(\w+)\s*\{")


def mask_rust(source: str) -> str:
    """Blank out comments and string bodies, keeping every offset and newline.

    Brace counting on raw Rust source is wrong: `format!("agent.{}", id)` closes
    a block that was never opened. Masking keeps the delimiters so a regex can
    still find `#[` and `fn`, and blanks what is between them.
    """
    out = list(source)
    i, n = 0, len(source)
    while i < n:
        c = source[i]
        if c == "/" and i + 1 < n and source[i + 1] == "/":
            j = source.find("\n", i)
            j = n if j < 0 else j
            for k in range(i, j):
                out[k] = " " if source[k] != "\n" else "\n"
            i = j
            continue
        if c == "/" and i + 1 < n and source[i + 1] == "*":
            j = source.find("*/", i + 2)
            j = n if j < 0 else j + 2
            for k in range(i, j):
                out[k] = " " if source[k] != "\n" else "\n"
            i = j
            continue
        if c == "r" and i + 1 < n and source[i + 1] in '#"':
            k = i + 1
            hashes = 0
            while k < n and source[k] == "#":
                hashes += 1
                k += 1
            if k < n and source[k] == '"':
                closing = '"' + "#" * hashes
                j = source.find(closing, k + 1)
                j = n if j < 0 else j + len(closing)
                for m in range(i, j):
                    out[m] = " " if source[m] != "\n" else "\n"
                i = j
                continue
        if c == '"':
            j = i + 1
            while j < n:
                if source[j] == "\\":
                    j += 2
                    continue
                if source[j] == '"':
                    j += 1
                    break
                j += 1
            for k in range(i + 1, min(j, n) - 1):
                out[k] = " " if source[k] != "\n" else "\n"
            i = j
            continue
        if c == "'":
            # A lifetime (`'py`) is not a char literal; a char literal closes
            # within four characters.
            for width in (3, 4):
                if i + width - 1 < n and source[i + width - 1] == "'":
                    for k in range(i + 1, i + width - 1):
                        out[k] = " "
                    i = i + width
                    break
            else:
                i += 1
            continue
        i += 1
    return "".join(out)


def _balanced_slice(masked: str, start: int, opening: str, closing: str) -> tuple[int, int]:
    """Offsets of the group opening at or after `start`, closing included."""
    begin = masked.find(opening, start)
    if begin < 0:
        return -1, -1
    depth = 0
    for i in range(begin, len(masked)):
        if masked[i] == opening:
            depth += 1
        elif masked[i] == closing:
            depth -= 1
            if depth == 0:
                return begin, i + 1
    return begin, -1


def split_top_level(text: str) -> list[str]:
    """Split on commas that are not nested in brackets."""
    parts, depth, current = [], 0, []
    for ch in text:
        if ch in "([{<":
            depth += 1
        elif ch in ")]}>":
            depth -= 1
        if ch == "," and depth == 0:
            parts.append("".join(current).strip())
            current = []
            continue
        current.append(ch)
    tail = "".join(current).strip()
    if tail:
        parts.append(tail)
    return [p for p in parts if p]


@dataclass
class Param:
    name: str
    kind: str = "normal"  # normal | vararg | kwarg
    kw_only: bool = False
    has_default: bool = False


@dataclass
class Member:
    name: str
    params: list[Param] = field(default_factory=list)
    is_property: bool = False
    line: int = 0


@dataclass
class BridgeClass:
    struct: str
    python_name: str
    path: str
    line: int
    members: dict[str, Member] = field(default_factory=dict)
    set_aside: list[str] = field(default_factory=list)
    uninterpreted: list[str] = field(default_factory=list)


def parse_signature_attribute(text: str) -> list[Param]:
    """Parse the parenthesised list of a `#[pyo3(signature = (...))]`."""
    params: list[Param] = []
    kw_only = False
    for entry in split_top_level(text):
        if entry == "*":
            kw_only = True
            continue
        if entry == "/":
            continue
        if entry.startswith("**"):
            params.append(Param(entry[2:].strip(), kind="kwarg"))
            continue
        if entry.startswith("*"):
            params.append(Param(entry[1:].strip(), kind="vararg"))
            kw_only = True
            continue
        name, _, default = entry.partition("=")
        params.append(
            Param(name.strip(), kw_only=kw_only, has_default=bool(default.strip()))
        )
    return params


def parse_fn_parameters(text: str) -> tuple[list[Param], bool]:
    """Parse a Rust `fn` parameter list; second value flags an `Option<T>`.

    An `Option<T>` with no declared signature is a *required* parameter in
    PyO3 0.24, not one defaulting to `None`. The guard does not guess: it hands
    the case back as uninterpreted.
    """
    params: list[Param] = []
    saw_option = False
    for entry in split_top_level(text):
        stripped = entry.replace("&", "").replace("mut ", "").strip()
        if stripped == "self" or stripped.startswith("self:"):
            continue
        name, _, rust_type = entry.partition(":")
        name = name.strip().lstrip("&").replace("mut ", "").strip()
        if not name or name == "self":
            continue
        if "Python<" in rust_type or rust_type.strip().startswith("Python"):
            continue
        if re.search(r"\bOption\s*<", rust_type):
            saw_option = True
        params.append(Param(name))
    return params, saw_option


def read_bridge_classes(root: Path):
    """Every `#[pymethods] impl X` block under `root`, with its members.

    Returns the classes, the problems met while reading them, the fields of
    every struct, the Python name of every `#[pyclass]`, and the raw body of
    every `#[pymethods]` block.
    """
    classes: list[BridgeClass] = []
    problems: list[str] = []
    python_names: dict[str, str] = {}
    struct_fields: dict[str, dict[str, str]] = {}
    bodies: dict[str, str] = {}

    for path in sorted(root.rglob("*.rs")):
        source = path.read_text(encoding="utf-8")
        masked = mask_rust(source)
        rel = str(path.relative_to(REPO_ROOT)) if path.is_relative_to(REPO_ROOT) else path.name

        for match in PYCLASS_ATTR.finditer(source):
            options, struct = match.group(1) or "", match.group(2)
            named = PYCLASS_NAME.search(options)
            python_names[struct] = named.group(1) if named else struct

        for match in STRUCT_BLOCK.finditer(masked):
            struct = match.group(1)
            begin, end = _balanced_slice(masked, match.end() - 1, "{", "}")
            if end < 0:
                continue
            fields: dict[str, str] = {}
            for line in source[begin + 1 : end - 1].splitlines():
                stripped = line.strip()
                if stripped.startswith(("//", "#[")) or ":" not in stripped:
                    continue
                name, _, rust_type = stripped.partition(":")
                name = name.replace("pub", "").strip()
                if re.fullmatch(r"\w+", name):
                    fields[name] = rust_type.rstrip(",").strip()
            struct_fields.setdefault(struct, {}).update(fields)

        for match in PYMETHODS_BLOCK.finditer(masked):
            struct = match.group(1)
            begin, end = _balanced_slice(masked, match.end(), "{", "}")
            if end < 0:
                problems.append(f"{rel}: unterminated #[pymethods] block for {struct}")
                continue
            entry = BridgeClass(
                struct=struct,
                python_name=python_names.get(struct, struct),
                path=rel,
                line=source.count("\n", 0, match.start()) + 1,
            )
            _read_members(entry, source, masked, begin, end)
            bodies[struct] = source[begin:end]
            classes.append(entry)

    for entry in classes:
        entry.python_name = python_names.get(entry.struct, entry.struct)
    return classes, problems, struct_fields, python_names, bodies


def _read_members(entry: BridgeClass, source: str, masked: str, begin: int, end: int) -> None:
    """Walk one `#[pymethods]` body top-down, item by item, at depth one."""
    lines = source.splitlines(keepends=True)
    masked_lines = masked.splitlines(keepends=True)
    offsets, running = [], 0
    for line in lines:
        offsets.append(running)
        running += len(line)

    first = source.count("\n", 0, begin)
    last = source.count("\n", 0, end)
    depth = 0
    pending: list[str] = []
    index = first
    while index <= last and index < len(lines):
        raw, cooked = lines[index], masked_lines[index]
        stripped = raw.strip()
        if depth == 1 and stripped:
            if stripped.startswith("//"):
                index += 1
                continue
            if stripped.startswith("#["):
                start = offsets[index] + raw.index("#[")
                _, close = _balanced_slice(masked, start, "[", "]")
                if close > 0:
                    pending.append(source[start:close])
                    consumed = source.count("\n", start, close)
                    index += consumed + 1
                    continue
            head = FN_HEAD.match(raw)
            if head:
                _add_member(entry, source, masked, offsets[index], head.group(1), pending, index + 1)
                pending = []
            elif not stripped.startswith(("///", "//")):
                pending = []
        depth += cooked.count("{") - cooked.count("}")
        index += 1


def _add_member(
    entry: BridgeClass,
    source: str,
    masked: str,
    line_start: int,
    name: str,
    attributes: list[str],
    line_number: int,
) -> None:
    joined = " ".join(attributes)
    if "#[new]" in joined:
        entry.set_aside.append(f"{name} (#[new])")
        return
    if name in SPECIAL_BRIDGE_MEMBERS:
        entry.set_aside.append(f"{name} (special method)")
        return
    is_property = "#[getter]" in joined
    if "#[setter]" in joined:
        entry.set_aside.append(f"{name} (#[setter])")
        return

    signature = SIGNATURE_ATTR.search(joined)
    if signature:
        open_paren = joined.index("(", signature.end() - 1)
        _, close = _balanced_slice(joined, open_paren, "(", ")")
        params = parse_signature_attribute(joined[open_paren + 1 : close - 1])
    else:
        head = masked.index("fn", line_start)
        open_paren, close = _balanced_slice(masked, head, "(", ")")
        params, saw_option = parse_fn_parameters(source[open_paren + 1 : close - 1])
        if saw_option:
            entry.uninterpreted.append(
                f"{entry.python_name}.{name} at {entry.path}:{line_number}: an "
                "Option<T> parameter with no #[pyo3(signature = ...)]"
            )
            return
    entry.members[name] = Member(name=name, params=params, is_property=is_property, line=line_number)


# ── Python reading ───────────────────────────────────────────────────────────


@dataclass
class ProtocolClass:
    name: str
    module: str
    members: dict[str, Member] = field(default_factory=dict)


def _protocol_params(node: ast.AST) -> list[Param]:
    args = node.args
    params: list[Param] = []
    positional = list(args.posonlyargs) + list(args.args)
    defaults = list(args.defaults)
    first_default = len(positional) - len(defaults)
    for rank, arg in enumerate(positional):
        if arg.arg == "self":
            continue
        params.append(Param(arg.arg, has_default=rank >= first_default))
    if args.vararg is not None:
        params.append(Param(args.vararg.arg, kind="vararg"))
    for arg, default in zip(args.kwonlyargs, args.kw_defaults):
        params.append(Param(arg.arg, kw_only=True, has_default=default is not None))
    if args.kwarg is not None:
        params.append(Param(args.kwarg.arg, kind="kwarg"))
    return params


def read_protocol(sdk_root: Path) -> tuple[list[tuple[str, str]], dict[str, str], dict[str, ProtocolClass]]:
    """Ctx services, the type-to-module map, and every context protocol class."""
    types_module = ast.parse((sdk_root / "types.py").read_text(encoding="utf-8"))
    services: list[tuple[str, str]] = []
    type_to_module: dict[str, str] = {}
    for node in types_module.body:
        if isinstance(node, ast.ClassDef) and node.name == "Ctx":
            for item in node.body:
                if isinstance(item, ast.AnnAssign) and isinstance(item.target, ast.Name):
                    services.append((item.target.id, ast.unparse(item.annotation)))
        if isinstance(node, ast.ImportFrom) and node.module and node.module.startswith(
            "apollia.context."
        ):
            stem = node.module.split(".")[-1]
            for alias in node.names:
                type_to_module[alias.name] = stem

    classes: dict[str, ProtocolClass] = {}
    for path in sorted((sdk_root / "context").glob("*.py")):
        if path.name == "__init__.py":
            continue
        module = ast.parse(path.read_text(encoding="utf-8"))
        for node in module.body:
            if not isinstance(node, ast.ClassDef):
                continue
            entry = ProtocolClass(name=node.name, module=path.stem)
            for item in node.body:
                if not isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    continue
                if item.name.startswith("_"):
                    continue
                is_property = any(
                    (isinstance(d, ast.Name) and d.id == "property")
                    or (isinstance(d, ast.Attribute) and d.attr == "property")
                    for d in item.decorator_list
                )
                entry.members[item.name] = Member(
                    name=item.name,
                    params=[] if is_property else _protocol_params(item),
                    is_property=is_property,
                    line=item.lineno,
                )
            classes[node.name] = entry
    return services, type_to_module, classes


# ── the crossing ─────────────────────────────────────────────────────────────


@dataclass
class Crossing:
    services: list[str] = field(default_factory=list)
    service_pairs: dict[str, tuple[str, str]] = field(default_factory=dict)
    nullable: dict[str, bool] = field(default_factory=dict)
    detached: dict[str, bool] = field(default_factory=dict)
    class_pairs: int = 0
    member_pairs: int = 0
    divergences: list[str] = field(default_factory=list)
    uninterpreted: list[str] = field(default_factory=list)
    stale_exceptions: list[str] = field(default_factory=list)
    set_aside: int = 0

    @property
    def clean(self) -> bool:
        return not (self.divergences or self.uninterpreted or self.stale_exceptions)


def _describe(param: Param) -> str:
    marks = []
    if param.kind == "vararg":
        return f"*{param.name}"
    if param.kind == "kwarg":
        return f"**{param.name}"
    if param.kw_only:
        marks.append("keyword-only")
    if param.has_default:
        marks.append("with a default")
    return param.name + (f" [{', '.join(marks)}]" if marks else "")


def _compare_members(label: str, published: Member, bound: Member) -> list[str]:
    out: list[str] = []
    if published.is_property != bound.is_property:
        published_kind = "a property" if published.is_property else "a method"
        bound_kind = "a property" if bound.is_property else "a method"
        out.append(f"{label}: the protocol publishes {published_kind}, the bridge binds {bound_kind}")
        return out
    if len(published.params) != len(bound.params):
        out.append(
            f"{label}: the protocol publishes {len(published.params)} parameter(s) "
            f"[{', '.join(_describe(p) for p in published.params)}], the bridge binds "
            f"{len(bound.params)} [{', '.join(_describe(p) for p in bound.params)}]"
        )
        return out
    for rank, (left, right) in enumerate(zip(published.params, bound.params)):
        if left.name != right.name or left.kind != right.kind:
            out.append(
                f"{label}: parameter at rank {rank} is `{_describe(left)}` in the "
                f"protocol and `{_describe(right)}` in the bridge"
            )
            continue
        if left.kw_only != right.kw_only:
            side = "the protocol" if left.kw_only else "the bridge"
            out.append(
                f"{label}: parameter `{left.name}` at rank {rank} is keyword-only "
                f"in {side} only"
            )
        if left.has_default != right.has_default:
            side = "the protocol" if left.has_default else "the bridge"
            out.append(
                f"{label}: parameter `{left.name}` at rank {rank} carries a default "
                f"in {side} only"
            )
    return out


def _bridge_class_for_service(
    accessor_body: str,
    struct_fields: dict[str, dict[str, str]],
    by_struct: dict[str, BridgeClass],
) -> str | None:
    """Resolve which `#[pymethods]` class an accessor hands back.

    Two steps, field first because it is the more precise: the field the body
    reads, whose declared type names the struct, and failing that the first
    `#[pymethods]` struct the body constructs by name.
    """
    fields = struct_fields.get("RuntimeContext", {})
    for field_name in SELF_FIELD.findall(accessor_body):
        declared = fields.get(field_name)
        if not declared:
            continue
        for candidate in re.findall(r"\w+", declared):
            if candidate in by_struct:
                return candidate
    for candidate in re.findall(r"\w+", accessor_body):
        if candidate in by_struct and candidate != "RuntimeContext":
            return candidate
    return None


def cross(sdk_root: Path, bridge_root: Path, declared: Declared = REAL_TREE) -> Crossing:
    """Build the junction of the two halves. The only reader of the bridge."""
    crossing = Crossing()
    services, type_to_module, protocol_classes = read_protocol(sdk_root)
    classes, problems, struct_fields, python_names, bodies = read_bridge_classes(bridge_root)
    crossing.uninterpreted.extend(problems)

    by_struct = {entry.struct: entry for entry in classes}
    root = by_struct.get("RuntimeContext")
    if root is None or not services:
        return crossing

    # Level 1, the services.
    published = {name for name, _ in services}
    bound = set(root.members)
    for name in sorted(published & bound):
        crossing.services.append(name)
    for name in sorted(published - bound):
        crossing.divergences.append(
            f"ctx.{name}: published by the protocol, absent from #[pymethods] impl RuntimeContext"
        )
    for name in sorted(bound - published):
        if name in declared.ctx_members_the_protocol_does_not_publish:
            crossing.set_aside += 1
            continue
        crossing.divergences.append(
            f"ctx.{name}: bound by the bridge, published by no annotation on Ctx"
        )
    for name in declared.ctx_members_the_protocol_does_not_publish:
        if name not in bound:
            crossing.stale_exceptions.append(
                f"ctx.{name} is declared unpublished but the bridge no longer binds it"
            )

    root_body = bodies.get("RuntimeContext", "")
    accessors = _split_accessors(root_body)
    accessor_docs = _split_accessor_docs(root_body)
    service_types = dict(services)

    # Level 2, the classes.
    for name in crossing.services:
        crossing.nullable[name] = "py.None()" in accessors.get(name, "")
        verdict = _attachment_verdict(accessor_docs.get(name, ""))
        if crossing.nullable[name] and verdict is None:
            crossing.divergences.append(
                f"ctx.{name}: the accessor can hand back None and its doc comment "
                "carries no `ctx-attachment:` verdict, so nothing says whether a "
                "production path reaches that branch"
            )
        elif verdict is not None and verdict not in ATTACHMENT_VERDICTS:
            crossing.uninterpreted.append(
                f"ctx.{name}: `ctx-attachment: {verdict}` is not one of "
                f"{ATTACHMENT_VERDICTS}"
            )
        elif verdict == "optional" and not crossing.nullable[name]:
            crossing.divergences.append(
                f"ctx.{name}: declared `ctx-attachment: optional`, but the accessor "
                "carries no branch handing back None, so the absence it claims "
                "cannot happen"
            )
        crossing.detached[name] = crossing.nullable[name] and verdict == "optional"
        annotation = service_types[name]
        published_type = annotation.replace("| None", "").replace("|None", "").strip()
        if "None" in annotation and not crossing.detached[name]:
            crossing.divergences.append(
                f"ctx.{name}: the protocol annotates `{annotation}`, but the bridge "
                "never leaves this service unattached"
            )
        if name in declared.services_with_no_bridge_class:
            crossing.set_aside += 1
            continue
        struct = _bridge_class_for_service(accessors.get(name, ""), struct_fields, by_struct)
        if struct is None:
            crossing.uninterpreted.append(
                f"ctx.{name}: no #[pymethods] class could be resolved from the accessor"
            )
            continue
        entry = by_struct[struct]
        crossing.service_pairs[name] = (published_type, entry.python_name)
        crossing.class_pairs += 1
        if entry.python_name != published_type:
            crossing.divergences.append(
                f"ctx.{name}: the protocol publishes the class `{published_type}`, the "
                f"bridge exposes `{entry.python_name}` ({entry.path}:{entry.line})"
            )
        published_class = protocol_classes.get(published_type)
        if published_class is None:
            crossing.uninterpreted.append(
                f"ctx.{name}: no protocol class named `{published_type}` was read"
            )
            continue
        crossing.member_pairs += _cross_members(crossing, name, published_class, entry)

    for name in declared.services_with_no_bridge_class:
        if name not in crossing.services:
            crossing.stale_exceptions.append(
                f"ctx.{name} is declared classless but Ctx no longer publishes it"
            )

    # Every class on either side is paired, or declared outside the crossing.
    crossed_structs = {
        by_struct[s].struct
        for s in (
            _bridge_class_for_service(accessors.get(n, ""), struct_fields, by_struct)
            for n in crossing.services
        )
        if s
    }
    for entry in classes:
        if entry.struct in crossed_structs or entry.struct == "RuntimeContext":
            continue
        if entry.python_name in declared.bridge_classes_outside_the_crossing:
            crossing.set_aside += 1
            continue
        crossing.uninterpreted.append(
            f"{entry.path}:{entry.line}: #[pymethods] impl {entry.struct} is neither "
            "crossed nor declared outside the crossing"
        )
    crossed_protocol = {t for t, _ in crossing.service_pairs.values()}
    for name, entry in protocol_classes.items():
        if name in crossed_protocol:
            continue
        if name in declared.protocol_classes_outside_the_crossing:
            crossing.set_aside += 1
            continue
        crossing.uninterpreted.append(
            f"sdk/apollia/context/{entry.module}.py: protocol class `{name}` is "
            "neither crossed nor declared outside the crossing"
        )
    for name in declared.bridge_classes_outside_the_crossing:
        if name not in {e.python_name for e in classes}:
            crossing.stale_exceptions.append(
                f"the bridge class `{name}` is declared outside the crossing but no "
                "longer carries a #[pymethods] block"
            )
    for name in declared.protocol_classes_outside_the_crossing:
        if name not in protocol_classes:
            crossing.stale_exceptions.append(
                f"the protocol class `{name}` is declared outside the crossing but no "
                "longer exists"
            )
    return crossing


def _cross_members(
    crossing: Crossing, service: str, published: ProtocolClass, bound: BridgeClass
) -> int:
    pairs = 0
    crossing.uninterpreted.extend(bound.uninterpreted)
    crossing.set_aside += len(bound.set_aside)
    for name in sorted(set(published.members) & set(bound.members)):
        pairs += 1
        crossing.divergences.extend(
            _compare_members(
                f"ctx.{service}.{name}", published.members[name], bound.members[name]
            )
        )
    for name in sorted(set(published.members) - set(bound.members)):
        crossing.divergences.append(
            f"ctx.{service}.{name}: protocol-only member, no #[pymethods] entry binds it "
            f"({published.module}.py:{published.members[name].line})"
        )
    for name in sorted(set(bound.members) - set(published.members)):
        crossing.divergences.append(
            f"ctx.{service}.{name}: bridge-only member, the protocol publishes no such "
            f"method ({bound.path}:{bound.members[name].line})"
        )
    return pairs


def _attachment_verdict(doc: str) -> str | None:
    """The `ctx-attachment:` verdict an accessor's doc comment carries, if any."""
    match = ATTACHMENT.search(doc)
    return match.group(1) if match else None


def _split_accessor_docs(body: str) -> dict[str, str]:
    """Slice a `#[pymethods]` body into one doc-comment block per `fn`.

    `_split_accessors` cuts at the `fn` line, so the doc comment of an accessor
    lands in the slice of the accessor before it. The attachment verdict lives
    in that doc comment, so it needs its own cut, taken forward: the run of
    `///` lines that immediately precedes a `fn`, attributes and blank lines
    passed through.
    """
    out: dict[str, str] = {}
    buffer: list[str] = []
    for line in body.splitlines():
        stripped = line.strip()
        if stripped.startswith("///"):
            buffer.append(stripped)
            continue
        head = FN_HEAD.match(line)
        if head:
            out[head.group(1)] = "\n".join(buffer)
            buffer = []
            continue
        if stripped.startswith("#[") or not stripped:
            continue
        buffer = []
    return out


def _split_accessors(body: str) -> dict[str, str]:
    """Slice a `#[pymethods]` body into one text per `fn`, name to source."""
    masked = mask_rust(body)
    out: dict[str, str] = {}
    starts: list[tuple[int, str]] = []
    for match in re.finditer(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)", masked, re.M):
        starts.append((match.start(), match.group(1)))
    for index, (offset, name) in enumerate(starts):
        stop = starts[index + 1][0] if index + 1 < len(starts) else len(body)
        out[name] = body[offset:stop]
    return out


# ── report ───────────────────────────────────────────────────────────────────

def report(sdk_root: Path, bridge_root: Path, declared: Declared = REAL_TREE) -> int:
    crossing = cross(sdk_root, bridge_root, declared)
    print(f"services paired               : {len(crossing.services)}")
    print(f"classes paired                : {crossing.class_pairs}")
    print(f"member pairs compared         : {crossing.member_pairs}")
    print(f"members set aside by a rule   : {crossing.set_aside}")
    print(f"divergences                   : {len(crossing.divergences)}")
    for line in crossing.divergences:
        print(f"  {line}")
    print(f"left uninterpreted            : {len(crossing.uninterpreted)}")
    for line in crossing.uninterpreted:
        print(f"  {line}")
    print(f"declared exceptions now stale : {len(crossing.stale_exceptions)}")
    for line in crossing.stale_exceptions:
        print(f"  {line}")
    nullable = sorted(name for name, flag in crossing.nullable.items() if flag)
    print(f"accessors that can hand back None: {len(nullable)} {nullable}")
    detached = sorted(name for name, flag in crossing.detached.items() if flag)
    print(f"services the bridge really leaves unattached: {len(detached)} {detached}")
    print()

    if len(crossing.services) < declared.service_floor:
        print(
            f"NOTHING MEASURED: {len(crossing.services)} service(s) paired, "
            f"fewer than the floor of {declared.service_floor}."
        )
        return 2
    faults = []
    if crossing.divergences:
        faults.append(f"{len(crossing.divergences)} divergence(s)")
    if crossing.uninterpreted:
        faults.append(f"{len(crossing.uninterpreted)} item(s) left uninterpreted")
    if crossing.stale_exceptions:
        faults.append(f"{len(crossing.stale_exceptions)} stale declared exception(s)")
    if faults:
        print(f"FAIL: {'; '.join(faults)}.")
        return 1
    print("OK: the protocol and the bridge agree on every service, class and member")
    return 0


# ── self-test ────────────────────────────────────────────────────────────────


def _write(root: Path, relative: str, body: str) -> None:
    target = root / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(body, encoding="utf-8")


SUBJECT_TYPES = '''"""Fabricated Ctx for the self-test."""

from apollia.context.thing import ThingInterface


class Ctx:
    thing: ThingInterface
'''

SUBJECT_PROTOCOL = '''"""ctx.thing - fabricated service."""

from typing import Protocol


class ThingInterface(Protocol):
    """A fabricated service."""

    async def push(self, name: str, *, tag: str | None = None) -> None:
        """Push a thing."""
        ...

    @property
    def count(self) -> int:
        """How many."""
        ...
'''

SUBJECT_BRIDGE = """#[pyclass(name = "ThingInterface")]
pub struct PyThingInterface {
    count: i64,
}

#[pymethods]
impl PyThingInterface {
    /// Push a thing.
    #[pyo3(signature = (name, *, tag=None))]
    fn push(&self, py: Python<'_>, name: String, tag: Option<String>) -> PyResult<()> {
        let _ = format!("a brace in a string: {} {{", name);
        Ok(())
    }

    /// How many.
    #[getter]
    fn count(&self) -> i64 {
        self.count
    }
}
"""

SUBJECT_CONTEXT = """#[pyclass(name = "RuntimeContext")]
pub struct RuntimeContext {
    thing: Option<pyo3::Py<crate::thing::PyThingInterface>>,
}

#[pymethods]
impl RuntimeContext {
    /// The fabricated service.
    ///
    /// ctx-attachment: optional, the fabricated constructor leaves it unset.
    #[getter]
    fn thing(&self, py: Python<'_>) -> PyObject {
        match &self.thing {
            Some(t) => t.clone_ref(py).into_any(),
            None => py.None(),
        }
    }
}
"""


def _case(name: str, condition: bool) -> bool:
    print(f"  {'ok  ' if condition else 'FAIL'}  {name}")
    return condition


def selftest() -> int:
    print("ctx contract crossing: both directions on a fabricated subject")
    root = Path(tempfile.mkdtemp(prefix="check-ctx-contract-"))
    try:
        sdk_root, bridge_root = root / "sdk" / "apollia", root / "bridge"

        def measure(protocol: str, bridge: str, context: str = "") -> tuple[str, int]:
            shutil.rmtree(sdk_root, ignore_errors=True)
            shutil.rmtree(bridge_root, ignore_errors=True)
            _write(sdk_root, "types.py", SUBJECT_TYPES)
            _write(sdk_root, "context/thing.py", protocol)
            _write(bridge_root, "thing.rs", bridge)
            _write(bridge_root, "context.rs", context or SUBJECT_CONTEXT)
            buffer = io.StringIO()
            with contextlib.redirect_stdout(buffer):
                code = report(sdk_root, bridge_root, FABRICATED_SUBJECT)
            return buffer.getvalue(), code

        results: list[bool] = []

        text, code = measure(SUBJECT_PROTOCOL, SUBJECT_BRIDGE)
        results.append(
            _case(
                "positive control: the repaired subject crosses clean",
                code == 0 and "member pairs compared         : 2" in text,
            )
        )

        text, code = measure(
            SUBJECT_PROTOCOL,
            SUBJECT_BRIDGE.replace(
                "    /// How many.",
                "    /// Extra.\n"
                "    fn drain(&self) -> i64 {\n        0\n    }\n\n    /// How many.",
            ),
        )
        results.append(
            _case(
                "direction 1: a bridge method the protocol does not publish is a red",
                code == 1 and "ctx.thing.drain: bridge-only member" in text,
            )
        )

        text, code = measure(
            SUBJECT_PROTOCOL.replace(
                "    @property",
                "    async def drain(self) -> int:\n"
                '        """Drain."""\n'
                "        ...\n\n"
                "    @property",
            ),
            SUBJECT_BRIDGE,
        )
        results.append(
            _case(
                "direction 2: a published method the bridge does not implement is a red",
                code == 1 and "ctx.thing.drain: protocol-only member" in text,
            )
        )

        text, code = measure(
            SUBJECT_PROTOCOL.replace("name: str", "label: str"), SUBJECT_BRIDGE
        )
        results.append(
            _case(
                "a renamed parameter is a red naming the rank and both names",
                code == 1
                and "parameter at rank 0 is `label` in the protocol and `name` in the bridge"
                in text,
            )
        )

        text, code = measure(
            SUBJECT_PROTOCOL, SUBJECT_BRIDGE.replace("(name, *, tag=None)", "(name, tag=None)")
        )
        results.append(
            _case(
                "a `*` on one side only is a red",
                code == 1 and "is keyword-only in the protocol only" in text,
            )
        )

        text, code = measure(
            SUBJECT_PROTOCOL.replace("tag: str | None = None", "tag: str | None"),
            SUBJECT_BRIDGE,
        )
        results.append(
            _case(
                "a default on one side only is a red",
                code == 1 and "carries a default in the bridge only" in text,
            )
        )

        text, code = measure(
            SUBJECT_PROTOCOL,
            SUBJECT_BRIDGE.replace(
                "    /// How many.\n    #[getter]",
                "    /// Unreadable.\n"
                "    fn sink(&self, tag: Option<String>) -> i64 {\n        0\n    }\n\n"
                "    /// How many.\n    #[getter]",
            ),
        )
        results.append(
            _case(
                "an Option<T> with no declared signature is left uninterpreted",
                code == 1 and "an Option<T> parameter with no #[pyo3(signature = ...)]" in text,
            )
        )

        text, code = measure(
            SUBJECT_PROTOCOL,
            SUBJECT_BRIDGE,
            SUBJECT_CONTEXT.replace(
                "    /// ctx-attachment: optional, the fabricated constructor leaves it unset.\n",
                "",
            ),
        )
        results.append(
            _case(
                "a null branch with no attachment verdict is a red",
                code == 1 and "carries no `ctx-attachment:` verdict" in text,
            )
        )

        text, code = measure(
            SUBJECT_PROTOCOL,
            SUBJECT_BRIDGE,
            SUBJECT_CONTEXT.replace(
                """        match &self.thing {
            Some(t) => t.clone_ref(py).into_any(),
            None => py.None(),
        }""",
                "        self.thing.clone_ref(py).into_any()",
            ),
        )
        results.append(
            _case(
                "an `optional` verdict on an accessor with no null branch is a red",
                code == 1 and "the absence it claims cannot happen" in text,
            )
        )

        shutil.rmtree(bridge_root, ignore_errors=True)
        _write(bridge_root, "empty.rs", "// nothing here\n")
        buffer = io.StringIO()
        with contextlib.redirect_stdout(buffer):
            empty = report(sdk_root, bridge_root, FABRICATED_SUBJECT)
        results.append(
            _case("a subject with no #[pymethods] block measures nothing", empty == 2)
        )

        print()
        if all(results):
            print(f"self-test: all {len(results)} cases pass")
            return 0
        print(f"self-test: {results.count(False)} of {len(results)} cases fail")
        return 1
    finally:
        shutil.rmtree(root, ignore_errors=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--selftest", action="store_true", help="replay the fixture controls instead of measuring the tree"
    )
    if parser.parse_args().selftest:
        sys.exit(selftest())
    sdk_root, bridge_root = REPO_ROOT / SDK_SUBTREE, REPO_ROOT / BRIDGE_SUBTREE
    for subtree in (sdk_root, bridge_root):
        if not subtree.is_dir():
            print(f"NOTHING MEASURED: {subtree} is absent from this tree.")
            sys.exit(2)
    sys.exit(report(sdk_root, bridge_root))


if __name__ == "__main__":
    main()
