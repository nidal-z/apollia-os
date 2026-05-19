"""Signature inference → JSON Schema + runtime payload validation.

This module is the heart of the SDK's "no boilerplate" promise: handler
parameters are introspected with ``inspect`` and ``typing``, then mapped
to a JSON Schema fragment which is later validated against incoming task
payloads.

Supported annotations:

- Primitives: ``str``, ``int``, ``float``, ``bool``, ``bytes``
- Containers: ``list[T]``, ``tuple[T, ...]``, ``dict[str, T]``
- ``Optional[T]`` / ``T | None`` → schema of ``T`` with ``nullable: true``
- ``T | U`` (non-Optional unions) → ``{"anyOf": [...]}``
- ``T | U | None`` → ``{"anyOf": [...], "nullable": true}``
- ``Literal[...]`` → ``{"enum": [...]}`` with inferred ``type``
- ``Any`` / missing annotation → ``{}`` (any-type)
- ``Annotated[T, "description"]`` → schema of ``T`` with ``description``
- dataclasses / TypedDict / NamedTuple → object schemas

Strictness:

- Payload validation does **not** silently coerce strings to numbers.
- Extra payload fields are rejected when ``additionalProperties`` is
  ``False`` (the default for inferred schemas).
- Validation accepts both the compact ``nullable: true`` form and the
  legacy ``"type": ["X", "null"]`` form for null acceptance — this keeps
  hand-crafted schemas working.
"""

from __future__ import annotations

import dataclasses
import difflib
import inspect
import typing
from collections.abc import Callable
from typing import Annotated, Any, Literal, Union, get_args, get_origin

from apollia.errors import PayloadError, SchemaError

__all__ = [
    "EXCLUDED_PARAM_NAMES",
    "EXCLUDED_TYPE_NAMES",
    "signature_to_input_schema",
    "annotation_to_schema",
    "return_to_output_schema",
    "validate_payload",
]

EXCLUDED_PARAM_NAMES: tuple[str, ...] = ("self", "cls", "ctx")
EXCLUDED_TYPE_NAMES: tuple[str, ...] = ("Ctx",)

# ``types.UnionType`` only exists on Python 3.10+, which is our minimum.
from types import UnionType as _UnionType


# ──────────────────────────────────────────────────────────────────────
# Schema inference
# ──────────────────────────────────────────────────────────────────────


def _is_none_type(tp: Any) -> bool:
    return tp is type(None)


def _is_union(origin: Any) -> bool:
    if origin is Union:
        return True
    if origin is _UnionType:
        return True
    return False


def _is_optional(annotation: Any) -> tuple[bool, Any]:
    """Return ``(is_optional, inner_type)``.

    For ``Optional[T]`` / ``T | None`` returns ``(True, T)``.
    For ``T | U | None`` returns ``(True, T | U)``.
    Otherwise returns ``(False, annotation)``.
    """
    origin = get_origin(annotation)
    if not _is_union(origin):
        return False, annotation
    args = [a for a in get_args(annotation) if not _is_none_type(a)]
    has_none = any(_is_none_type(a) for a in get_args(annotation))
    if not has_none:
        return False, annotation
    if len(args) == 1:
        return True, args[0]
    # Reconstruct a Union[...] without None.
    return True, Union[tuple(args)]


def _scalar_schema(tp: Any) -> dict[str, Any] | None:
    if tp is str:
        return {"type": "string"}
    if tp is bool:
        # bool is a subclass of int — check it FIRST.
        return {"type": "boolean"}
    if tp is int:
        return {"type": "integer"}
    if tp is float:
        return {"type": "number"}
    if tp is bytes:
        return {"type": "string", "format": "byte"}
    return None


def _dataclass_schema(tp: type) -> dict[str, Any]:
    properties: dict[str, Any] = {}
    required: list[str] = []
    try:
        hints = typing.get_type_hints(tp, include_extras=True)
    except Exception:
        hints = {}
    for field in dataclasses.fields(tp):
        ann = hints.get(field.name, field.type)
        properties[field.name] = annotation_to_schema(ann)
        has_default = (
            field.default is not dataclasses.MISSING
            or field.default_factory is not dataclasses.MISSING
        )
        if not has_default:
            required.append(field.name)
    schema: dict[str, Any] = {
        "type": "object",
        "properties": properties,
        "additionalProperties": False,
    }
    if required:
        schema["required"] = required
    return schema


def _typeddict_schema(tp: type) -> dict[str, Any]:
    properties: dict[str, Any] = {}
    try:
        hints = typing.get_type_hints(tp, include_extras=True)
    except Exception:
        hints = dict(getattr(tp, "__annotations__", {}))
    for name, ann in hints.items():
        properties[name] = annotation_to_schema(ann)
    required_keys = getattr(tp, "__required_keys__", frozenset(hints.keys()))
    schema: dict[str, Any] = {
        "type": "object",
        "properties": properties,
        "additionalProperties": False,
    }
    if required_keys:
        schema["required"] = sorted(required_keys)
    return schema


def _namedtuple_schema(tp: type) -> dict[str, Any]:
    fields: tuple[str, ...] = getattr(tp, "_fields", ())
    defaults: dict[str, Any] = getattr(tp, "_field_defaults", {})
    try:
        hints = typing.get_type_hints(tp, include_extras=True)
    except Exception:
        hints = {}
    properties: dict[str, Any] = {}
    required: list[str] = []
    for name in fields:
        ann = hints.get(name, Any)
        properties[name] = annotation_to_schema(ann)
        if name not in defaults:
            required.append(name)
    schema: dict[str, Any] = {
        "type": "object",
        "properties": properties,
        "additionalProperties": False,
    }
    if required:
        schema["required"] = required
    return schema


def _is_namedtuple(tp: Any) -> bool:
    return (
        isinstance(tp, type)
        and issubclass(tp, tuple)
        and hasattr(tp, "_fields")
        and hasattr(tp, "_field_defaults")
    )


def annotation_to_schema(annotation: Any) -> dict[str, Any]:
    """Convert a single type annotation to a JSON Schema fragment.

    Raises :class:`SchemaError` if the annotation is unsupported.
    """
    # Missing annotation or Any → any-type.
    if annotation is inspect.Parameter.empty or annotation is Any or annotation is None:
        if annotation is None:
            return {"type": "null"}
        return {}

    # Annotated[T, "description", ...]: unwrap T, collect string metadata as
    # description. Multiple string metadata entries are concatenated with a
    # space separator (most common use case: chained context strings).
    if get_origin(annotation) is Annotated:
        annotated_args = get_args(annotation)
        if annotated_args:
            inner_ann = annotated_args[0]
            descriptions = [m for m in annotated_args[1:] if isinstance(m, str) and m]
            inner_schema = annotation_to_schema(inner_ann)
            if descriptions:
                merged = dict(inner_schema)
                existing = merged.get("description")
                if isinstance(existing, str) and existing:
                    merged["description"] = " ".join([existing, *descriptions])
                else:
                    merged["description"] = " ".join(descriptions)
                return merged
            return inner_schema

    # Optional[T] / T | None.
    is_opt, inner = _is_optional(annotation)
    if is_opt:
        inner_schema = annotation_to_schema(inner)
        # Compact ``nullable: true`` form — more widely understood by LLMs
        # than the JSON-Schema-draft ``"type": ["X", "null"]`` array form,
        # and less verbose in the tool descriptor.
        new_schema = dict(inner_schema)
        new_schema["nullable"] = True
        return new_schema

    origin = get_origin(annotation)
    args = get_args(annotation)

    # Literal[...].
    if origin is Literal:
        enum_values = list(args)
        types_in_enum = {type(v) for v in enum_values}
        schema: dict[str, Any] = {"enum": enum_values}
        if types_in_enum == {str}:
            schema["type"] = "string"
        elif types_in_enum == {bool}:
            schema["type"] = "boolean"
        elif types_in_enum == {int}:
            schema["type"] = "integer"
        elif types_in_enum <= {int, float} and float in types_in_enum:
            schema["type"] = "number"
        return schema

    # Union (non-Optional).
    if _is_union(origin):
        return {"anyOf": [annotation_to_schema(a) for a in args]}

    # list[T] / List[T].
    if origin in (list, typing.List):  # noqa: UP006
        item_ann = args[0] if args else Any
        return {"type": "array", "items": annotation_to_schema(item_ann)}

    # tuple[T, ...] / Tuple[T, ...].
    if origin in (tuple, typing.Tuple):  # noqa: UP006
        if not args:
            return {"type": "array"}
        # Homogeneous tuple: Tuple[T, ...].
        if len(args) == 2 and args[1] is Ellipsis:
            return {"type": "array", "items": annotation_to_schema(args[0])}
        # Heterogeneous fixed-length tuple.
        return {
            "type": "array",
            "prefixItems": [annotation_to_schema(a) for a in args],
            "minItems": len(args),
            "maxItems": len(args),
        }

    # dict[str, T] / Dict[str, T].
    if origin in (dict, typing.Dict):  # noqa: UP006
        if len(args) == 2:
            key_ann, value_ann = args
            if key_ann is not str and key_ann is not Any:
                raise SchemaError(
                    f"Unsupported dict key type {key_ann!r}; only str keys are supported."
                )
            return {
                "type": "object",
                "additionalProperties": annotation_to_schema(value_ann),
            }
        return {"type": "object"}

    # Bare unparameterised generic builtins.
    if annotation is list:
        return {"type": "array"}
    if annotation is tuple:
        return {"type": "array"}
    if annotation is dict:
        return {"type": "object"}

    # Plain types from here on.
    if isinstance(annotation, type):
        scalar = _scalar_schema(annotation)
        if scalar is not None:
            return scalar
        if dataclasses.is_dataclass(annotation):
            return _dataclass_schema(annotation)
        if typing.is_typeddict(annotation):
            return _typeddict_schema(annotation)
        if _is_namedtuple(annotation):
            return _namedtuple_schema(annotation)

    raise SchemaError(f"Unsupported type annotation: {annotation!r}")


def _should_exclude_param(param: inspect.Parameter) -> bool:
    if param.name in EXCLUDED_PARAM_NAMES:
        return True
    ann = param.annotation
    if ann is inspect.Parameter.empty:
        return False
    # Match by type name "Ctx" (typing.Protocol classes).
    ann_name = getattr(ann, "__name__", None) or getattr(ann, "_name", None)
    if isinstance(ann_name, str) and ann_name in EXCLUDED_TYPE_NAMES:
        return True
    # Forward reference / stringified annotation.
    if isinstance(ann, str) and ann in EXCLUDED_TYPE_NAMES:
        return True
    return False


def signature_to_input_schema(fn: Callable[..., Any]) -> dict[str, Any]:
    """Generate a JSON Schema ``object`` from a function signature.

    Returns ``{"type": "object", "properties": {...}, "required": [...],
    "additionalProperties": False}``. Raises :class:`SchemaError` if any
    parameter has an unsupported annotation.
    """
    try:
        sig = inspect.signature(fn)
    except (TypeError, ValueError) as exc:
        raise SchemaError(f"Cannot inspect signature of {fn!r}: {exc}") from exc

    try:
        hints = typing.get_type_hints(fn, include_extras=True)
    except Exception:
        hints = {}

    properties: dict[str, Any] = {}
    required: list[str] = []

    for name, param in sig.parameters.items():
        if _should_exclude_param(param):
            continue
        if param.kind in (
            inspect.Parameter.VAR_POSITIONAL,
            inspect.Parameter.VAR_KEYWORD,
        ):
            # *args / **kwargs are not part of the JSON Schema surface.
            continue
        annotation = hints.get(name, param.annotation)
        properties[name] = annotation_to_schema(annotation)
        # A param is required iff it has no default AND is not Optional.
        is_opt, _ = _is_optional(annotation)
        if param.default is inspect.Parameter.empty and not is_opt:
            required.append(name)

    schema: dict[str, Any] = {
        "type": "object",
        "properties": properties,
        "additionalProperties": False,
    }
    if required:
        schema["required"] = required
    return schema


def return_to_output_schema(fn: Callable[..., Any]) -> dict[str, Any]:
    """Generate a JSON Schema for the return annotation.

    Returns ``{}`` for missing / ``Any`` / ``None`` annotations.
    """
    try:
        hints = typing.get_type_hints(fn, include_extras=True)
    except Exception:
        hints = {}
    ann = hints.get("return", inspect.Parameter.empty)
    if ann is inspect.Parameter.empty or ann is Any or ann is None or ann is type(None):
        return {}
    try:
        return annotation_to_schema(ann)
    except SchemaError:
        # The return schema is informational only; if it's unsupported we
        # gracefully fall back to "any" rather than failing the whole agent.
        return {}


# ──────────────────────────────────────────────────────────────────────
# Payload validation
# ──────────────────────────────────────────────────────────────────────


def _python_type_name(value: Any) -> str:
    """Human-friendly Python type name for error messages."""
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "boolean"
    if isinstance(value, int):
        return "integer"
    if isinstance(value, float):
        return "number"
    if isinstance(value, str):
        return "string"
    if isinstance(value, list):
        return "array"
    if isinstance(value, dict):
        return "object"
    return type(value).__name__


def _format_type_decl(type_decl: Any) -> str:
    """Render a JSON-Schema type declaration as a readable string."""
    if isinstance(type_decl, list):
        return " | ".join(str(t) for t in type_decl)
    return str(type_decl)


def _suggest(field_name: str, candidates: list[str]) -> str | None:
    """Return the closest matching candidate field name, or None."""
    if not candidates:
        return None
    matches = difflib.get_close_matches(field_name, candidates, n=1, cutoff=0.6)
    if matches:
        return matches[0]
    return None


def _matches_type(value: Any, type_decl: Any) -> bool:
    """Check that ``value`` matches a JSON-Schema ``type`` declaration."""
    if isinstance(type_decl, list):
        return any(_matches_type(value, t) for t in type_decl)
    if type_decl == "string":
        return isinstance(value, str)
    if type_decl == "integer":
        # Reject bool which is a subclass of int.
        return isinstance(value, int) and not isinstance(value, bool)
    if type_decl == "number":
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    if type_decl == "boolean":
        return isinstance(value, bool)
    if type_decl == "array":
        return isinstance(value, list)
    if type_decl == "object":
        return isinstance(value, dict)
    if type_decl == "null":
        return value is None
    return True


def _validate_value(value: Any, schema: dict[str, Any], path: str) -> None:
    """Validate ``value`` against ``schema``; raise PayloadError on mismatch."""
    if not schema:
        return  # Any-type, accept everything.

    # nullable: True allows None regardless of the declared ``type``.
    if value is None and schema.get("nullable") is True:
        return

    # anyOf.
    if "anyOf" in schema:
        last_err: PayloadError | None = None
        for sub in schema["anyOf"]:
            try:
                _validate_value(value, sub, path)
                return
            except PayloadError as exc:
                last_err = exc
        if last_err is not None:
            raise PayloadError(
                f"{path or '<root>'}: no anyOf branch matched ({last_err.message})",
                field=path or None,
            )
        return

    # enum.
    if "enum" in schema and value not in schema["enum"]:
        raise PayloadError(
            f"{path or '<root>'}: value {value!r} not in enum {schema['enum']}",
            field=path or None,
        )

    type_decl = schema.get("type")
    if type_decl is not None and not _matches_type(value, type_decl):
        actual_type = _python_type_name(value)
        raise PayloadError(
            f"{path or '<root>'}: expected type {_format_type_decl(type_decl)}, "
            f"got {actual_type}",
            field=path or None,
            details={
                "field": path or None,
                "expected_type": type_decl,
                "actual_type": actual_type,
            },
        )

    if type_decl == "array" or (
        isinstance(type_decl, list) and "array" in type_decl and isinstance(value, list)
    ):
        items_schema = schema.get("items")
        if isinstance(items_schema, dict) and isinstance(value, list):
            for idx, item in enumerate(value):
                _validate_value(item, items_schema, f"{path}[{idx}]" if path else f"[{idx}]")
        prefix_items = schema.get("prefixItems")
        if isinstance(prefix_items, list) and isinstance(value, list):
            for idx, sub_schema in enumerate(prefix_items):
                if idx < len(value):
                    _validate_value(
                        value[idx], sub_schema, f"{path}[{idx}]" if path else f"[{idx}]"
                    )

    if type_decl == "object" or (
        isinstance(type_decl, list) and "object" in type_decl and isinstance(value, dict)
    ):
        if isinstance(value, dict):
            properties = schema.get("properties", {})
            required = schema.get("required", [])
            additional = schema.get("additionalProperties", True)
            expected = list(properties.keys())
            for req in required:
                if req not in value:
                    req_path = f"{path}.{req}" if path else req
                    raise PayloadError(
                        f"Missing required field '{req_path}'. Required: {required}.",
                        field=req_path,
                        details={"missing": req, "required": list(required)},
                    )
            for key, sub_value in value.items():
                if key in properties:
                    sub_path = f"{path}.{key}" if path else key
                    _validate_value(sub_value, properties[key], sub_path)
                else:
                    if additional is False:
                        key_path = f"{path}.{key}" if path else key
                        suggestion = _suggest(key, expected)
                        msg = (
                            f"Unexpected field '{key_path}'. "
                            f"Expected fields: {expected}."
                        )
                        details: dict[str, Any] = {
                            "unexpected": key,
                            "expected": expected,
                        }
                        if suggestion is not None:
                            msg += f" Did you mean '{suggestion}'?"
                            details["did_you_mean"] = suggestion
                        raise PayloadError(msg, field=key_path, details=details)
                    if isinstance(additional, dict):
                        sub_path = f"{path}.{key}" if path else key
                        _validate_value(sub_value, additional, sub_path)


def validate_payload(payload: dict[str, Any], schema: dict[str, Any]) -> dict[str, Any]:
    """Validate ``payload`` against ``schema`` and return coerced kwargs.

    The schema is expected to be the object schema produced by
    :func:`signature_to_input_schema`.

    Behaviour:

    - Missing required fields raise :class:`PayloadError` with ``field``.
    - Extra fields raise :class:`PayloadError` when
      ``additionalProperties`` is ``False`` (the default).
    - Type mismatches raise :class:`PayloadError` — no implicit
      string-to-number coercion.
    - Returns a shallow copy of ``payload`` suitable for ``**call``.
    """
    if not isinstance(payload, dict):
        raise PayloadError(
            f"payload must be a dict, got {type(payload).__name__}",
        )

    properties = schema.get("properties", {})
    required = schema.get("required", [])
    additional = schema.get("additionalProperties", True)
    expected = list(properties.keys())

    # Check required fields first so the error is deterministic.
    for req in required:
        if req not in payload:
            raise PayloadError(
                f"Missing required field '{req}'. Required: {required}.",
                field=req,
                details={"missing": req, "required": list(required)},
            )

    # Reject extra fields up-front if strict.
    if additional is False:
        for key in payload:
            if key not in properties:
                suggestion = _suggest(key, expected)
                msg = (
                    f"Unexpected field '{key}'. Expected fields: {expected}."
                )
                details: dict[str, Any] = {
                    "unexpected": key,
                    "expected": expected,
                }
                if suggestion is not None:
                    msg += f" Did you mean '{suggestion}'?"
                    details["did_you_mean"] = suggestion
                raise PayloadError(msg, field=key, details=details)

    # Validate each known field against its property schema.
    coerced: dict[str, Any] = {}
    for key, value in payload.items():
        if key in properties:
            _validate_value(value, properties[key], key)
        coerced[key] = value

    return coerced
