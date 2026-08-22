#!/usr/bin/env python3
"""Generate the SDK / ctx reference from the source of truth.

The reference is derived from ``sdk/apollia/types.py`` (the ``Ctx`` protocol and
the multi-modal content types) and the per-service protocols under
``sdk/apollia/context/*.py``. Nothing is hand-copied: this script parses the
Python source with the stdlib ``ast`` module (no import, no third-party
dependency) and renders one Markdown page per service plus an index.

The protocol is only half the contract. What an agent actually reaches is the
``#[pyclass(name = "RuntimeContext")]`` of ``crates/apollia-aip``, and for a
long time these pages published the half nobody runs: twenty-two divergences
between the two halves shipped as fact. ``scripts/check_ctx_contract.py`` is the
single engine that computes their junction, and this generator is one of its two
consumers. It calls it *before writing anything*: on a divergence it prints the
same list the guard prints, writes no page at all, and exits 1. The seventeen
pages stay in their last coherent state.

Publishing the divergence on the page instead was considered and refused. It
would turn a contract into a report on itself, the mark would be committed and
would read as a normal state, and above all it would offer a way to ship a
divergence by writing prose around it.

Run via ``docs/site/regen.sh`` (or ``python3 docs/site/scripts/gen_sdk_ref.py``).
"""

import ast
import copy
import sys
from pathlib import Path

HEADER = "<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->"

REPO_ROOT = Path(__file__).resolve().parents[3]
SDK_ROOT = REPO_ROOT / "sdk" / "apollia"
TYPES_PY = SDK_ROOT / "types.py"
CONTEXT_DIR = SDK_ROOT / "context"
OUT_DIR = REPO_ROOT / "docs" / "site" / "docs" / "reference" / "sdk"
BRIDGE_ROOT = REPO_ROOT / "crates" / "apollia-aip" / "src"

sys.path.insert(0, str(REPO_ROOT / "scripts"))

import check_ctx_contract  # noqa: E402

# Published on the services the bridge declares `ctx-attachment: optional`, and
# on those only. Deriving it from the presence of a `None =>` arm in the
# accessor put it on seven more pages whose service the bridge documents as
# always attached, which told an agent author to branch on an absence that
# cannot happen. The verdict is the bridge's, read by `check_ctx_contract`.
MAY_BE_ABSENT = (
    "The bridge may leave this service unattached; `ctx.{attr}` is then `None`."
)


def cell(text: str) -> str:
    """Escape a table cell so a pipe inside it does not open a new column.

    GFM cuts a cell on a bare `|` even inside a code span, and every cell past
    the header width is dropped. Ten published rows carried a truncated type
    and a wrong default because of it: `AIPResult.text` read "type `str`,
    default `None`" where the type is `str | None` and there is no default.
    """
    return text.replace("|", r"\|")


def first_line(text: str | None) -> str:
    """First non-empty line of a docstring, for one-line summaries."""
    if not text:
        return ""
    for line in text.strip().splitlines():
        line = line.strip()
        if line:
            return line
    return ""


def method_signature(node: ast.AST) -> str:
    """Reconstruct a ``def``/``async def`` signature line from an AST node."""
    stub = copy.deepcopy(node)
    stub.body = [ast.Expr(value=ast.Constant(value=Ellipsis))]
    stub.decorator_list = []
    ast.fix_missing_locations(stub)
    rendered = ast.unparse(stub)
    return rendered.splitlines()[0].rstrip(":")


def is_property(node: ast.AST) -> bool:
    """True when the function is decorated with `@property`.

    A `@property` reads as an attribute, not a call. Rendering one as a method
    signature tells the reader to write `ctx.budget.steps_remaining()`, which
    raises `TypeError` on an `int`. The decorator is stripped before unparsing
    (see `method_signature`), so it has to be detected here, before that.
    """
    for decorator in getattr(node, "decorator_list", []):
        if isinstance(decorator, ast.Name) and decorator.id == "property":
            return True
        if isinstance(decorator, ast.Attribute) and decorator.attr == "property":
            return True
    return False


def class_members(cls: ast.ClassDef):
    """Split a class body into (methods, annotated attributes).

    Properties are counted as attributes, with their return annotation as type.
    """
    methods = []
    attributes = []
    for item in cls.body:
        if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
            if item.name.startswith("_"):
                continue
            if is_property(item):
                annotation = (
                    ast.unparse(item.returns) if item.returns is not None else "Any"
                )
                attributes.append((item.name, annotation, None))
                continue
            methods.append(item)
        elif isinstance(item, ast.AnnAssign) and isinstance(item.target, ast.Name):
            name = item.target.id
            annotation = ast.unparse(item.annotation)
            default = ast.unparse(item.value) if item.value is not None else None
            attributes.append((name, annotation, default))
    return methods, attributes


def base_names(cls: ast.ClassDef) -> list[str]:
    names = []
    for base in cls.bases:
        try:
            names.append(ast.unparse(base))
        except Exception:
            pass
    return names


def render_class(cls: ast.ClassDef) -> str:
    """Render a single class (protocol, TypedDict or dataclass) as Markdown."""
    lines: list[str] = []
    bases = ", ".join(base_names(cls)) or "object"
    lines.append(f"### `{cls.name}`")
    lines.append("")
    lines.append(f"_Bases: {bases}_")
    lines.append("")
    doc = ast.get_docstring(cls)
    if doc:
        lines.append(doc.strip())
        lines.append("")

    methods, attributes = class_members(cls)

    if attributes:
        lines.append("| Field | Type | Default |")
        lines.append("| --- | --- | --- |")
        for name, annotation, default in attributes:
            default_cell = f"`{cell(default)}`" if default is not None else ""
            lines.append(f"| `{cell(name)}` | `{cell(annotation)}` | {default_cell} |")
        lines.append("")

    for method in methods:
        lines.append(f"#### `{method.name}`")
        lines.append("")
        lines.append("```python")
        lines.append(method_signature(method))
        lines.append("```")
        lines.append("")
        mdoc = ast.get_docstring(method)
        if mdoc:
            lines.append(mdoc.strip())
            lines.append("")
    return "\n".join(lines)


def parse_module(path: Path) -> ast.Module:
    return ast.parse(path.read_text(encoding="utf-8"), filename=str(path))


def public_classes(module: ast.Module) -> list[ast.ClassDef]:
    return [n for n in module.body if isinstance(n, ast.ClassDef)]


def public_functions(module: ast.Module) -> list[ast.AST]:
    """Module-level functions that are not private.

    `types.py` exports the constructors for multi-modal content next to the
    classes they build. Filtering the module body down to `ClassDef` dropped
    them, so a page titled "Content types and helpers" documented no helper.
    """
    return [
        n
        for n in module.body
        if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef))
        and not n.name.startswith("_")
    ]


def render_function(node: ast.AST) -> str:
    """Render a module-level function the way a method is rendered."""
    lines = [f"### `{node.name}`", "", "```python", method_signature(node), "```", ""]
    doc = ast.get_docstring(node)
    if doc:
        lines.append(doc.strip())
        lines.append("")
    return "\n".join(lines)


def module_alias(module: ast.Module, name: str) -> str | None:
    """Right-hand side of a module-level ``name = <expr>`` binding, if any.

    Some ``Ctx`` services are not protocols but aliases onto a stdlib type
    (``ctx.logger`` is ``logging.Logger``). Without this the page for such a
    service renders as "source not resolved", which is a generator gap rather
    than a real absence of contract.
    """
    for node in module.body:
        if isinstance(node, ast.Assign):
            targets = [t.id for t in node.targets if isinstance(t, ast.Name)]
            if name in targets:
                return ast.unparse(node.value)
        elif isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
            if node.target.id == name and node.value is not None:
                return ast.unparse(node.value)
    return None


def base_type(annotation: str) -> str:
    """The protocol class an annotation names, `| None` set aside.

    `Ctx.profile` is annotated `ProfileInterface | None` because one production
    context leaves it unattached. The union is what the index publishes; the
    bare name is what resolves the module and the class.
    """
    return annotation.replace("| None", "").replace("|None", "").strip()


def build_service_index(types_mod: ast.Module):
    """Extract the ordered Ctx service attributes and their import modules."""
    # attr name -> type name, in declaration order
    services: list[tuple[str, str]] = []
    for node in types_mod.body:
        if isinstance(node, ast.ClassDef) and node.name == "Ctx":
            _, attributes = class_members(node)
            services = [(name, annotation) for name, annotation, _ in attributes]

    # type name -> module stem, from `from apollia.context.<stem> import <Type>`
    type_to_module: dict[str, str] = {}
    for node in types_mod.body:
        if isinstance(node, ast.ImportFrom) and node.module:
            if node.module.startswith("apollia.context."):
                stem = node.module.split(".")[-1]
                for alias in node.names:
                    type_to_module[alias.name] = stem
    return services, type_to_module


def write_page(rel_name: str, frontmatter: dict, body: str) -> Path:
    fm_lines = ["---"]
    for key, value in frontmatter.items():
        fm_lines.append(f"{key}: {value}")
    fm_lines.append("---")
    content = "\n".join(fm_lines) + "\n" + HEADER + "\n\n" + body.rstrip() + "\n"
    out = OUT_DIR / rel_name
    out.write_text(content, encoding="utf-8")
    return out


def main() -> int:
    crossing = check_ctx_contract.cross(SDK_ROOT, BRIDGE_ROOT)
    measured = len(crossing.services)
    if measured < check_ctx_contract.REAL_TREE.service_floor:
        print(
            f"gen_sdk_ref: NOTHING MEASURED, {measured} service(s) crossed, fewer "
            f"than the floor of {check_ctx_contract.REAL_TREE.service_floor}. "
            "No page written."
        )
        return 1
    if not crossing.clean:
        print(
            "gen_sdk_ref: the protocol and the bridge diverge, so there is no "
            "contract to publish. No page written."
        )
        for line in (
            crossing.divergences + crossing.uninterpreted + crossing.stale_exceptions
        ):
            print(f"  {line}")
        print("Run python3 scripts/check_ctx_contract.py for the full crossing.")
        return 1

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    types_mod = parse_module(TYPES_PY)
    services, type_to_module = build_service_index(types_mod)

    # Cache parsed context modules and their classes by type name.
    module_classes: dict[str, list[ast.ClassDef]] = {}
    module_asts: dict[str, ast.Module] = {}
    class_by_name: dict[str, tuple[str, ast.ClassDef]] = {}
    for path in sorted(CONTEXT_DIR.glob("*.py")):
        if path.name == "__init__.py":
            continue
        mod = parse_module(path)
        classes = public_classes(mod)
        module_classes[path.stem] = classes
        module_asts[path.stem] = mod
        for cls in classes:
            class_by_name[cls.name] = (path.stem, cls)

    written: list[str] = []

    # One page per Ctx service, in declaration order.
    doc_summary: dict[str, str] = {}
    for position, (attr, annotation) in enumerate(services, start=1):
        type_name = base_type(annotation)
        stem = type_to_module.get(type_name)
        classes = module_classes.get(stem, []) if stem else []
        primary = next((c for c in classes if c.name == type_name), None)
        body_parts = [f"# `ctx.{attr}`", ""]
        if primary is not None:
            summary = first_line(ast.get_docstring(primary))
            doc_summary[attr] = summary
            body_parts.append(f"Service type: `{annotation}` (from `apollia.context.{stem}`).")
            if crossing.detached.get(attr):
                body_parts.append("")
                body_parts.append(MAY_BE_ABSENT.format(attr=attr))
            body_parts.append("")
            body_parts.append(render_class(primary))
            # Companion public classes in the same module (e.g. LlmResponse).
            for cls in classes:
                if cls.name != type_name:
                    body_parts.append(render_class(cls))
        else:
            alias = module_alias(module_asts[stem], type_name) if stem else None
            if alias is not None:
                mod_doc = ast.get_docstring(module_asts[stem])
                body_parts.append(
                    f"Service type: `{annotation}`, an alias for `{alias}`"
                    f" (from `apollia.context.{stem}`)."
                )
                if crossing.detached.get(attr):
                    body_parts.append("")
                    body_parts.append(MAY_BE_ABSENT.format(attr=attr))
                body_parts.append("")
                doc_summary[attr] = first_line(mod_doc)
                if mod_doc:
                    body_parts.append(mod_doc.strip())
            else:
                body_parts.append(f"Service type: `{annotation}` (source not resolved).")
        page = write_page(
            f"{attr}.md",
            {"sidebar_position": position, "title": f"ctx.{attr}"},
            "\n".join(body_parts),
        )
        written.append(page.name)

    # Multi-modal content types + helpers + legacy AIPResult, from types.py.
    typed_dicts = [
        c
        for c in public_classes(types_mod)
        if c.name != "Ctx"
    ]
    body_parts = [
        "# Content types and helpers",
        "",
        "Multi-modal content blocks, message shapes, and the legacy `AIPResult`,"
        " defined in `sdk/apollia/types.py`.",
        "",
    ]
    for cls in typed_dicts:
        body_parts.append(render_class(cls))

    helpers = public_functions(types_mod)
    if helpers:
        body_parts.append("## Helpers")
        body_parts.append("")
        for fn in helpers:
            body_parts.append(render_function(fn))

    write_page(
        "content-types.md",
        {"sidebar_position": len(services) + 1, "title": "Content types and helpers"},
        "\n".join(body_parts),
    )
    written.append("content-types.md")

    # Index page: the Ctx docstring + one row per service declared on Ctx.
    ctx_cls = next(
        (n for n in types_mod.body if isinstance(n, ast.ClassDef) and n.name == "Ctx"),
        None,
    )
    ctx_doc = ast.get_docstring(ctx_cls) if ctx_cls else ""
    index_parts = ["# SDK / ctx contract", ""]
    if ctx_doc:
        index_parts.append(ctx_doc.strip())
        index_parts.append("")
    index_parts.append("## Services")
    index_parts.append("")
    index_parts.append("| Service | Type | Summary |")
    index_parts.append("| --- | --- | --- |")
    for attr, type_name in services:
        summary = doc_summary.get(attr, "")
        index_parts.append(
            f"| [`ctx.{cell(attr)}`](./{attr}.md) | `{cell(type_name)}`"
            f" | {cell(summary)} |"
        )
    index_parts.append("")
    index_parts.append(
        "See also [Content types and helpers](./content-types.md) for the"
        " multi-modal message shapes."
    )
    write_page(
        "index.md",
        {"sidebar_position": 0, "title": "SDK / ctx contract"},
        "\n".join(index_parts),
    )

    print(
        f"gen_sdk_ref: wrote {len(written) + 1} pages to {OUT_DIR} "
        f"({measured} services crossed against the bridge, no divergence)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
