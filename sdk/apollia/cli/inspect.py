"""``apollia inspect`` - load an agent module and display its generated manifest.

The command performs read-only introspection: it loads the target ``.py``
file as a module, extracts the canonical manifest produced by ``@agent``
(or falls back to a legacy ``manifest()`` method/function), and renders
either a human-readable summary or a JSON document.
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


# Sentinel string used in the human-readable report for empty / absent fields.
_NONE_LABEL = "(none)"


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
        # Restore any pre-existing entry under the same name (paranoia -
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


def _prop_type_label(prop: Any) -> str:
    """Pick a short type label out of one JSON Schema property dict."""
    if not isinstance(prop, dict):
        return "any"
    raw_type = prop.get("type")
    if isinstance(raw_type, str):
        return raw_type
    if isinstance(raw_type, list) and raw_type:
        return "|".join(str(t) for t in raw_type)
    return "any"


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
        return t if isinstance(t, str) else _NONE_LABEL
    raw_required = schema.get("required")
    required: set[str] = (
        {r for r in raw_required if isinstance(r, str)}
        if isinstance(raw_required, list)
        else set()
    )
    parts = [
        f"{pname}: {_prop_type_label(prop)}{'!' if pname in required else '?'}"
        for pname, prop in props.items()
    ]
    return "{" + ", ".join(parts) + "}"


def _fmt_list(values: list[Any]) -> str:
    return ", ".join(str(v) for v in values) if values else _NONE_LABEL


def _header_lines(manifest: dict[str, Any]) -> list[str]:
    """Render the boxed header block describing the agent identity."""
    name = manifest.get("name", "?")
    version = manifest.get("version", "?")
    desc = manifest.get("description", "?")
    tags = _fmt_list(manifest.get("tags") or [])
    packages = _fmt_list(manifest.get("packages") or [])
    execution = manifest.get("execution_mode", "direct")
    supports_a2a = bool(manifest.get("supports_a2a", False))

    width = 65
    header = "─" * (width - 17)
    return [
        f"╭─ Apollia Agent {header}╮",
        f"│ Name:         {name}",
        f"│ Version:      {version}",
        f"│ Description:  {desc}",
        f"│ Tags:         {tags}",
        f"│ Packages:     {packages}",
        f"│ Execution:    {execution}",
        f"│ Supports A2A: {str(supports_a2a).lower()}",
        "╰" + "─" * width + "╯",
        "",
    ]


def _skill_lines(skill: dict[str, Any]) -> list[str]:
    """Render one skill entry (head + input/output + flags)."""
    sid = skill.get("id", "?")
    sdesc = skill.get("description", "") or ""
    head = f"  • {sid}"
    if sdesc:
        head += f" - {sdesc}"
    lines = [
        head,
        f"    Input:  {_format_schema_brief(skill.get('input_schema', {}))}",
        f"    Output: {_format_schema_brief(skill.get('output_schema', {}))}",
    ]
    if skill.get("requires_approval"):
        lines.append("    [HITL] Requires human approval")
    if skill.get("dangerous"):
        lines.append("    [!] Dangerous skill")
    return lines


def _skills_block(skills: list[dict[str, Any]]) -> list[str]:
    if not skills:
        return ["Skills: (none)", ""]
    lines = [f"Skills ({len(skills)}):"]
    for s in skills:
        lines.extend(_skill_lines(s))
    lines.append("")
    return lines


def _resources_block(manifest: dict[str, Any]) -> list[str]:
    ds_list = manifest.get("datasources") or []
    tpl_list = manifest.get("templates") or []
    secrets_list = manifest.get("secrets") or []
    tools_list = manifest.get("tools_required") or []
    return [
        "Declared resources:",
        f"  Datasources: {_fmt_list(ds_list)}",
        f"  Templates:   {_fmt_list(tpl_list)}",
        f"  Secrets:     {_fmt_list(secrets_list)}",
        "",
        f"Tools required: {_fmt_list(tools_list)}",
        "",
    ]


def _diagnostics_block(warnings: list[str], errors: list[str]) -> list[str]:
    lines: list[str] = []
    if warnings:
        lines.append("Warnings:")
        lines.extend(f"  • {w}" for w in warnings)
        lines.append("")
    if errors:
        lines.append("Errors:")
        lines.extend(f"  • {e}" for e in errors)
        lines.append("")
        lines.append("Status: invalid")
    else:
        lines.append("Status: valid")
    return lines


def _format_human(
    manifest: dict[str, Any],
    skills: list[dict[str, Any]],
    warnings: list[str],
    errors: list[str],
) -> str:
    """Build the human-readable inspect report."""
    lines: list[str] = []
    lines.extend(_header_lines(manifest))
    lines.extend(_skills_block(skills))
    lines.extend(_resources_block(manifest))
    lines.extend(_diagnostics_block(warnings, errors))
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


def _emit_error(
    json_mode: bool,
    *,
    error_text: str,
    stderr_text: str,
    traceback_text: str | None = None,
) -> None:
    """Print an inspect-error report in either JSON or human form."""
    if json_mode:
        payload: dict[str, Any] = {
            "manifest": {},
            "skills": [],
            "warnings": [],
            "errors": [error_text],
        }
        if traceback_text is not None:
            payload["traceback"] = traceback_text
        print(json.dumps(payload, indent=2))
    else:
        print(f"✗ {stderr_text}", file=sys.stderr)


def _load_module_for_inspect(
    path: Path, json_mode: bool
) -> tuple[ModuleType | None, int]:
    """Load the agent module; return ``(module, exit_code)``.

    ``exit_code`` is ``0`` on success (caller continues) or a non-zero
    code the caller should return as-is after the helper printed the
    error report.
    """
    try:
        return _load_agent_module(path), 0
    except FileNotFoundError as exc:
        _emit_error(json_mode, error_text=str(exc), stderr_text=str(exc))
        return None, 2
    except ValueError as exc:
        _emit_error(json_mode, error_text=str(exc), stderr_text=str(exc))
        return None, 2
    except Exception as exc:  # noqa: BLE001 - surface every load error
        message = f"Failed to load module: {exc}"
        _emit_error(
            json_mode,
            error_text=message,
            stderr_text=message,
            traceback_text=traceback.format_exc(),
        )
        return None, 1


def inspect_command(args: argparse.Namespace) -> int:
    """Execute ``apollia inspect`` and return a process exit code.

    Exit codes:
        - ``0`` - success
        - ``1`` - load failure or invalid manifest
        - ``2`` - argument / path error (file missing, wrong suffix)
    """
    json_mode = bool(getattr(args, "json", False))
    path = Path(args.agent_path).resolve()

    module, rc = _load_module_for_inspect(path, json_mode)
    if module is None:
        return rc

    try:
        manifest, skills, warnings = _extract_agent_data(module)
    except AgentError as exc:
        _emit_error(
            json_mode,
            error_text=str(exc),
            stderr_text=f"Inspection failed: {exc}",
        )
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
            "would see. Read-only - no task is executed."
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
