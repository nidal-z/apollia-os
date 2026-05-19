"""``apollia inspect`` — load an agent module and display its generated manifest.

The command performs read-only introspection: it loads the target ``.py``
file as a module, extracts the canonical manifest produced by ``@agent``
(or falls back to a legacy ``manifest()`` method/function), and renders
either a human-readable summary or a JSON document.

See ADR-110 for the design rationale (fail-fast manifest inspection,
parity with the runtime loader, exit code conventions).
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
import traceback
from pathlib import Path
from types import ModuleType
from typing import Any

from apollia.errors import AgentConfigError, AgentError

__all__ = [
    "build_parser",
    "inspect_command",
]


# ──────────────────────────────────────────────────────────────────────
# Module loading
# ──────────────────────────────────────────────────────────────────────


def _load_agent_module(path: Path) -> ModuleType:
    """Load the agent module from a ``.py`` path.

    The parent directory is temporarily prepended to ``sys.path`` so that
    relative imports inside the agent file resolve. The entry is removed
    on the way out (even on failure) to keep the interpreter state clean.

    Args:
        path: Absolute path to the agent ``.py`` file.

    Returns:
        The loaded module object.

    Raises:
        FileNotFoundError: when ``path`` does not exist.
        ValueError: when ``path`` is not a ``.py`` file.
        ImportError: when the spec cannot be created.
    """
    if not path.exists():
        raise FileNotFoundError(f"File not found: {path}")
    if path.suffix != ".py":
        raise ValueError(f"Expected a .py file, got: {path}")

    parent = str(path.parent)
    inserted_path = False
    if parent not in sys.path:
        sys.path.insert(0, parent)
        inserted_path = True

    module_name = f"_apollia_inspect_{path.stem}"
    previous_module = sys.modules.get(module_name)
    try:
        spec = importlib.util.spec_from_file_location(module_name, path)
        if spec is None or spec.loader is None:
            raise ImportError(f"Could not load spec for {path}")
        module = importlib.util.module_from_spec(spec)
        # Register before exec so decorators using ``inspect.getmodule(cls)``
        # (e.g. ``@agent`` exposing the singleton) can resolve the module.
        sys.modules[module_name] = module
        try:
            spec.loader.exec_module(module)
        except Exception:
            sys.modules.pop(module_name, None)
            raise
        return module
    finally:
        if inserted_path and parent in sys.path:
            sys.path.remove(parent)
        # Restore any pre-existing entry under the same name (paranoia —
        # we use a private prefix so collisions are unlikely).
        if previous_module is not None:
            sys.modules[module_name] = previous_module


# ──────────────────────────────────────────────────────────────────────
# Data extraction
# ──────────────────────────────────────────────────────────────────────


def _coerce_manifest(raw: Any) -> dict[str, Any]:
    """Coerce a legacy ``manifest()`` return value into a plain dict.

    Raises :class:`AgentConfigError` when the value is not a mapping.
    """
    if isinstance(raw, dict):
        return raw
    raise AgentConfigError(
        f"manifest() returned a {type(raw).__name__}, expected dict"
    )


def _extract_agent_data(
    module: ModuleType,
) -> tuple[dict[str, Any], list[dict[str, Any]], list[str]]:
    """Extract ``(manifest, skills, warnings)`` from a loaded module.

    Supports both the modern ``@agent``-decorated style (manifest cached
    on the class as ``__apollia_manifest__``) and the legacy style where
    the module exposes either an ``agent`` singleton with a ``manifest()``
    method, or a top-level ``manifest()`` function.

    Raises:
        AgentConfigError: when no manifest source can be identified.
    """
    warnings: list[str] = []

    agent_obj = getattr(module, "agent", None)

    if agent_obj is not None:
        cls = type(agent_obj)
        cached = getattr(cls, "__apollia_manifest__", None)
        if isinstance(cached, dict):
            skills_registry = getattr(cls, "__apollia_skills__", {}) or {}
            skills: list[dict[str, Any]] = []
            for skill_id, entry in skills_registry.items():
                skills.append(
                    {
                        "id": skill_id,
                        "description": getattr(entry, "description", ""),
                        "input_schema": getattr(entry, "input_schema", {}),
                        "output_schema": getattr(entry, "output_schema", {}),
                        "requires_approval": getattr(
                            entry, "requires_approval", False
                        ),
                        "dangerous": getattr(entry, "dangerous", False),
                    }
                )
            return cached, skills, warnings

        # Legacy path: instance with a manifest() method.
        manifest_attr = getattr(agent_obj, "manifest", None)
        if callable(manifest_attr):
            warnings.append(
                "Agent is not declared with @agent (legacy manifest() method); "
                "skill schemas may be incomplete."
            )
            manifest = _coerce_manifest(manifest_attr())
            skills_legacy = _skills_from_legacy_manifest(manifest)
            return manifest, skills_legacy, warnings

    # Legacy path: top-level manifest() function.
    top_manifest = getattr(module, "manifest", None)
    if callable(top_manifest):
        warnings.append(
            "No `agent` singleton found; using top-level manifest() function "
            "(legacy layout)."
        )
        manifest = _coerce_manifest(top_manifest())
        skills_legacy = _skills_from_legacy_manifest(manifest)
        return manifest, skills_legacy, warnings

    raise AgentConfigError(
        "Module exposes neither an `agent` singleton with a manifest, nor a "
        "top-level manifest() function."
    )


def _skills_from_legacy_manifest(manifest: dict[str, Any]) -> list[dict[str, Any]]:
    """Best-effort extraction of skills from a legacy manifest dict.

    Legacy manifests embed a ``skills`` list with at minimum ``id`` /
    ``name`` / ``description`` entries; input/output schemas are usually
    absent.
    """
    out: list[dict[str, Any]] = []
    raw_skills = manifest.get("skills") or []
    if not isinstance(raw_skills, list):
        return out
    for entry in raw_skills:
        if not isinstance(entry, dict):
            continue
        sid = entry.get("id") or entry.get("name") or ""
        if not sid:
            continue
        out.append(
            {
                "id": sid,
                "description": entry.get("description", ""),
                "input_schema": entry.get("input_schema", {}),
                "output_schema": entry.get("output_schema", {}),
                "requires_approval": bool(entry.get("requires_approval", False)),
                "dangerous": bool(entry.get("dangerous", False)),
            }
        )
    return out


# ──────────────────────────────────────────────────────────────────────
# Output formatting
# ──────────────────────────────────────────────────────────────────────


def _format_schema_brief(schema: Any) -> str:
    """Render a one-line summary of a JSON-Schema-shaped dict.

    Falls back to ``(any)`` when the schema is not a typed ``object`` with
    a ``properties`` map.
    """
    if not isinstance(schema, dict):
        return "(any)"
    props = schema.get("properties")
    if not isinstance(props, dict) or not props:
        # Fall back to the schema's own type if it's a primitive.
        t = schema.get("type")
        if isinstance(t, str):
            return t
        return "(none)"
    required: set[str] = set()
    raw_required = schema.get("required")
    if isinstance(raw_required, list):
        required = {r for r in raw_required if isinstance(r, str)}
    parts: list[str] = []
    for pname, prop in props.items():
        ptype = "any"
        if isinstance(prop, dict):
            raw_type = prop.get("type")
            if isinstance(raw_type, str):
                ptype = raw_type
            elif isinstance(raw_type, list) and raw_type:
                ptype = "|".join(str(t) for t in raw_type)
        marker = "!" if pname in required else "?"
        parts.append(f"{pname}: {ptype}{marker}")
    return "{" + ", ".join(parts) + "}"


def _format_human(
    manifest: dict[str, Any],
    skills: list[dict[str, Any]],
    warnings: list[str],
    errors: list[str],
) -> str:
    """Build the human-readable inspect report."""
    lines: list[str] = []

    name = manifest.get("name", "?")
    version = manifest.get("version", "?")
    desc = manifest.get("description", "?")
    tags_list = manifest.get("tags") or []
    packages_list = manifest.get("packages") or []
    execution = manifest.get("execution_mode", "direct")
    supports_a2a = bool(manifest.get("supports_a2a", False))

    tags = ", ".join(str(t) for t in tags_list) if tags_list else "(none)"
    packages = (
        ", ".join(str(p) for p in packages_list) if packages_list else "(none)"
    )

    width = 65
    header = "─" * (width - 17)
    lines.append(f"╭─ Apollia Agent {header}╮")
    lines.append(f"│ Name:         {name}")
    lines.append(f"│ Version:      {version}")
    lines.append(f"│ Description:  {desc}")
    lines.append(f"│ Tags:         {tags}")
    lines.append(f"│ Packages:     {packages}")
    lines.append(f"│ Execution:    {execution}")
    lines.append(f"│ Supports A2A: {str(supports_a2a).lower()}")
    lines.append("╰" + "─" * width + "╯")
    lines.append("")

    if skills:
        lines.append(f"Skills ({len(skills)}):")
        for s in skills:
            sid = s.get("id", "?")
            sdesc = s.get("description", "") or ""
            head = f"  • {sid}"
            if sdesc:
                head += f" — {sdesc}"
            lines.append(head)
            ischema = s.get("input_schema", {})
            oschema = s.get("output_schema", {})
            lines.append(f"    Input:  {_format_schema_brief(ischema)}")
            lines.append(f"    Output: {_format_schema_brief(oschema)}")
            if s.get("requires_approval"):
                lines.append("    [HITL] Requires human approval")
            if s.get("dangerous"):
                lines.append("    [!] Dangerous skill")
        lines.append("")
    else:
        lines.append("Skills: (none)")
        lines.append("")

    ds_list = manifest.get("datasources") or []
    tpl_list = manifest.get("templates") or []
    secrets_list = manifest.get("secrets") or []
    tools_list = manifest.get("tools_required") or []

    def _fmt_list(values: list[Any]) -> str:
        return ", ".join(str(v) for v in values) if values else "(none)"

    lines.append("Declared resources:")
    lines.append(f"  Datasources: {_fmt_list(ds_list)}")
    lines.append(f"  Templates:   {_fmt_list(tpl_list)}")
    lines.append(f"  Secrets:     {_fmt_list(secrets_list)}")
    lines.append("")
    lines.append(f"Tools required: {_fmt_list(tools_list)}")
    lines.append("")

    if warnings:
        lines.append("Warnings:")
        for w in warnings:
            lines.append(f"  • {w}")
        lines.append("")

    if errors:
        lines.append("Errors:")
        for e in errors:
            lines.append(f"  • {e}")
        lines.append("")
        lines.append("Status: invalid")
    else:
        lines.append("Status: valid")

    return "\n".join(lines)


def _format_json(
    manifest: dict[str, Any],
    skills: list[dict[str, Any]],
    warnings: list[str],
    errors: list[str],
) -> str:
    """Build the machine-readable JSON inspect document."""
    payload: dict[str, Any] = {
        "manifest": manifest,
        "skills": skills,
        "warnings": warnings,
        "errors": errors,
    }
    return json.dumps(payload, indent=2, ensure_ascii=False, default=str)


# ──────────────────────────────────────────────────────────────────────
# Command entry point
# ──────────────────────────────────────────────────────────────────────


def inspect_command(args: argparse.Namespace) -> int:
    """Execute ``apollia inspect`` and return a process exit code.

    Exit codes:
        - ``0`` — success
        - ``1`` — load failure or invalid manifest
        - ``2`` — argument / path error (file missing, wrong suffix)
    """
    json_mode = bool(getattr(args, "json", False))
    path = Path(args.agent_path).resolve()

    try:
        module = _load_agent_module(path)
    except FileNotFoundError as exc:
        if json_mode:
            print(
                json.dumps(
                    {
                        "manifest": {},
                        "skills": [],
                        "warnings": [],
                        "errors": [str(exc)],
                    },
                    indent=2,
                )
            )
        else:
            print(f"✗ {exc}", file=sys.stderr)
        return 2
    except ValueError as exc:
        if json_mode:
            print(
                json.dumps(
                    {
                        "manifest": {},
                        "skills": [],
                        "warnings": [],
                        "errors": [str(exc)],
                    },
                    indent=2,
                )
            )
        else:
            print(f"✗ {exc}", file=sys.stderr)
        return 2
    except Exception as exc:  # noqa: BLE001 — surface every load error
        message = f"Failed to load module: {exc}"
        if json_mode:
            print(
                json.dumps(
                    {
                        "manifest": {},
                        "skills": [],
                        "warnings": [],
                        "errors": [message],
                        "traceback": traceback.format_exc(),
                    },
                    indent=2,
                )
            )
        else:
            print(f"✗ {message}", file=sys.stderr)
        return 1

    try:
        manifest, skills, warnings = _extract_agent_data(module)
    except AgentError as exc:
        message = f"Inspection failed: {exc}"
        if json_mode:
            print(
                json.dumps(
                    {
                        "manifest": {},
                        "skills": [],
                        "warnings": [],
                        "errors": [str(exc)],
                    },
                    indent=2,
                )
            )
        else:
            print(f"✗ {message}", file=sys.stderr)
        return 1

    if json_mode:
        print(_format_json(manifest, skills, warnings, []))
    else:
        print(_format_human(manifest, skills, warnings, []))
    return 0


# ──────────────────────────────────────────────────────────────────────
# Parser wiring
# ──────────────────────────────────────────────────────────────────────


def build_parser(subparsers: Any) -> argparse.ArgumentParser:
    """Register the ``inspect`` sub-command on a parent ``subparsers``.

    Returns the created parser so callers can extend it if needed.
    """
    p: argparse.ArgumentParser = subparsers.add_parser(
        "inspect",
        help="Inspect an agent module without running it",
        description=(
            "Load an agent .py file and display the manifest the runtime "
            "would see. Read-only — no task is executed."
        ),
    )
    p.add_argument("agent_path", help="Path to the agent .py file")
    p.add_argument(
        "--json",
        action="store_true",
        help="Emit a JSON document instead of the human-readable report.",
    )
    p.set_defaults(func=inspect_command)
    return p
