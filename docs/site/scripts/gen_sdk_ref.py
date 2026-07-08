#!/usr/bin/env python3
"""Generate the SDK / ctx reference from the source of truth.

The reference is derived from ``sdk/apollia/types.py`` (the ``Ctx`` protocol and
the multi-modal content types) and the per-service protocols under
``sdk/apollia/context/*.py``. Nothing is hand-copied: this script parses the
Python source with the stdlib ``ast`` module (no import, no third-party
dependency) and renders one Markdown page per service plus an index.

Run via ``docs/site/regen.sh`` (or ``python3 docs/site/scripts/gen_sdk_ref.py``).
"""

import ast
import copy
from pathlib import Path

HEADER = "<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->"

REPO_ROOT = Path(__file__).resolve().parents[3]
SDK_ROOT = REPO_ROOT / "sdk" / "apollia"
TYPES_PY = SDK_ROOT / "types.py"
CONTEXT_DIR = SDK_ROOT / "context"
OUT_DIR = REPO_ROOT / "docs" / "site" / "docs" / "reference" / "sdk"


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


def class_members(cls: ast.ClassDef):
    """Split a class body into (methods, annotated attributes)."""
    methods = []
    attributes = []
    for item in cls.body:
        if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
            if item.name.startswith("_"):
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
            default_cell = f"`{default}`" if default is not None else ""
            lines.append(f"| `{name}` | `{annotation}` | {default_cell} |")
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


def main() -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    types_mod = parse_module(TYPES_PY)
    services, type_to_module = build_service_index(types_mod)

    # Cache parsed context modules and their classes by type name.
    module_classes: dict[str, list[ast.ClassDef]] = {}
    class_by_name: dict[str, tuple[str, ast.ClassDef]] = {}
    for path in sorted(CONTEXT_DIR.glob("*.py")):
        if path.name == "__init__.py":
            continue
        mod = parse_module(path)
        classes = public_classes(mod)
        module_classes[path.stem] = classes
        for cls in classes:
            class_by_name[cls.name] = (path.stem, cls)

    written: list[str] = []

    # One page per Ctx service, in declaration order.
    doc_summary: dict[str, str] = {}
    for position, (attr, type_name) in enumerate(services, start=1):
        stem = type_to_module.get(type_name)
        classes = module_classes.get(stem, []) if stem else []
        primary = next((c for c in classes if c.name == type_name), None)
        body_parts = [f"# `ctx.{attr}`", ""]
        if primary is not None:
            summary = first_line(ast.get_docstring(primary))
            doc_summary[attr] = summary
            body_parts.append(f"Service type: `{type_name}` (from `apollia.context.{stem}`).")
            body_parts.append("")
            body_parts.append(render_class(primary))
            # Companion public classes in the same module (e.g. LlmResponse).
            for cls in classes:
                if cls.name != type_name:
                    body_parts.append(render_class(cls))
        else:
            body_parts.append(f"Service type: `{type_name}` (source not resolved).")
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
    write_page(
        "content-types.md",
        {"sidebar_position": len(services) + 1, "title": "Content types and helpers"},
        "\n".join(body_parts),
    )
    written.append("content-types.md")

    # Index page: the Ctx docstring + the 14-service table.
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
        index_parts.append(f"| [`ctx.{attr}`](./{attr}.md) | `{type_name}` | {summary} |")
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

    print(f"gen_sdk_ref: wrote {len(written) + 1} pages to {OUT_DIR}")


if __name__ == "__main__":
    main()
