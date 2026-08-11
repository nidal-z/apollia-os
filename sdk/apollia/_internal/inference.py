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
  legacy ``"type": ["X", "null"]`` form for null acceptance - this keeps
  hand-crafted schemas working.
"""

import dataclasses
import difflib
import inspect
import typing
import warnings
from collections.abc import Callable, Sequence

# ``types.UnionType`` only exists on Python 3.10+, which is our minimum.
from types import UnionType as _UnionType
from typing import Annotated, Any, Literal, Union, get_args, get_origin

from apollia.errors import PayloadError, SchemaError

__all__ = [
    "EXCLUDED_PARAM_NAMES",
    "EXCLUDED_TYPE_NAMES",
    "annotation_to_schema",
    "return_to_output_schema",
    "signature_to_input_schema",
    "validate_payload",
]

EXCLUDED_PARAM_NAMES: tuple[str, ...] = ("self", "cls", "ctx")
EXCLUDED_TYPE_NAMES: tuple[str, ...] = ("Ctx",)

# ``typing.NotRequired`` / ``typing.Required`` (Python 3.11+) are special forms
# that wrap a TypedDict field type. They carry no runtime schema information of
# their own - they only signal that the key is optional/required, which is
# already exposed via ``TypedDict.__required_keys__`` / ``__optional_keys__``.
# We unwrap them at the TypedDict-introspection layer so the SDK can produce a
# structural schema for the inner type ``T``.
try:  # pragma: no cover - both available on supported Pythons (3.11+)
    from typing import NotRequired as _NotRequired
    from typing import Required as _Required
except ImportError:  # pragma: no cover
    _NotRequired = None  # type: ignore[assignment]
    _Required = None  # type: ignore[assignment]


# ──────────────────────────────────────────────────────────────────────
# Schema inference
# ──────────────────────────────────────────────────────────────────────


def _is_none_type(tp: object) -> bool:
    return tp is type(None)


def _is_union(origin: object) -> bool:
    if origin is Union:
        return True
    return origin is _UnionType


def _is_optional(annotation: object) -> tuple[bool, Any]:
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
    # REASON(UP007): this is a runtime subscription over a computed tuple, not an
    # annotation. The PEP 604 form would need reduce(operator.or_, args), which
    # fails on any member that does not implement __or__; Union[...] accepts every
    # typing object the inference layer can be handed.
    return True, Union[tuple(args)]  # noqa: UP007


def _scalar_schema(tp: object) -> dict[str, Any] | None:
    if tp is str:
        return {"type": "string"}
    if tp is bool:
        # bool is a subclass of int - check it FIRST.
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
    # A payload TypedDict defined in a module that used
    # ``from __future__ import annotations`` (PEP 563) exposes unresolved
    # annotations (plain strings or ``ForwardRef`` wrappers, depending on the
    # Python version). That silently corrupts ``__required_keys__`` (Python
    # cannot see through ``NotRequired[...]`` when it is a string) and therefore
    # the generated skill schema. Surface it loudly rather than mis-deriving.
    raw_annotations = getattr(tp, "__annotations__", {})
    if any(isinstance(ann, (str, typing.ForwardRef)) for ann in raw_annotations.values()):
        warnings.warn(
            f"TypedDict {getattr(tp, '__name__', tp)!r} exposes stringified "
            "annotations, most likely from 'from __future__ import annotations'. "
            "Remove that import from the defining module: it breaks "
            "required/optional field derivation for agent skill schemas.",
            stacklevel=2,
        )

    try:
        hints = typing.get_type_hints(tp, include_extras=True)
        hints_resolved = True
    except Exception:
        # Annotations could not be resolved (forward refs, missing names). Fall
        # back to the raw annotations for the property schemas and to
        # ``__required_keys__`` for the required set below.
        hints = dict(raw_annotations)
        hints_resolved = False

    # ``__total__`` is the class-level default: total TypedDicts (the default)
    # make every key required unless wrapped in ``NotRequired``; ``total=False``
    # flips the default to optional unless wrapped in ``Required``.
    total = bool(getattr(tp, "__total__", True))
    properties: dict[str, Any] = {}
    derived_required: set[str] = set()
    for name, ann in hints.items():
        origin = get_origin(ann)
        if origin is _Required:
            required = True
            inner_args = get_args(ann)
            if inner_args:
                ann = inner_args[0]
        elif origin is _NotRequired:
            required = False
            inner_args = get_args(ann)
            if inner_args:
                ann = inner_args[0]
        else:
            required = total
        if required:
            derived_required.add(name)
        properties[name] = annotation_to_schema(ann)

    # Prefer the per-field derivation above (it honors ``NotRequired`` /
    # ``Required`` and ``__total__``, and stays correct even when the annotations
    # had to be recovered from strings). Only when ``get_type_hints`` failed
    # entirely do we defer to ``__required_keys__``.
    if hints_resolved:
        required_keys: set[str] = derived_required
    else:
        required_keys = set(getattr(tp, "__required_keys__", frozenset(hints.keys())))

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


def _is_namedtuple(tp: object) -> bool:
    return (
        isinstance(tp, type)
        and issubclass(tp, tuple)
        and hasattr(tp, "_fields")
        and hasattr(tp, "_field_defaults")
    )


def _schema_for_empty_annotation(annotation: object) -> dict[str, Any] | None:
    """Schema for ``empty``, ``Any``, or literal ``None`` annotations."""
    if annotation is inspect.Parameter.empty or annotation is Any:
        return {}
    if annotation is None:
        return {"type": "null"}
    return None


def _schema_for_annotated(annotation: object) -> dict[str, Any] | None:
    """Unwrap ``Annotated[T, "desc", ...]`` into the schema of ``T`` plus desc."""
    if get_origin(annotation) is not Annotated:
        return None
    annotated_args = get_args(annotation)
    if not annotated_args:
        return None
    inner_ann = annotated_args[0]
    descriptions = [m for m in annotated_args[1:] if isinstance(m, str) and m]
    inner_schema = annotation_to_schema(inner_ann)
    if not descriptions:
        return inner_schema
    merged = dict(inner_schema)
    existing = merged.get("description")
    if isinstance(existing, str) and existing:
        merged["description"] = " ".join([existing, *descriptions])
    else:
        merged["description"] = " ".join(descriptions)
    return merged


def _schema_for_literal(args: tuple[Any, ...]) -> dict[str, Any]:
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


def _schema_for_tuple(args: tuple[Any, ...]) -> dict[str, Any]:
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


def _schema_for_dict(args: tuple[Any, ...]) -> dict[str, Any]:
    if len(args) != 2:
        return {"type": "object"}
    key_ann, value_ann = args
    if key_ann is not str and key_ann is not Any:
        raise SchemaError(f"Unsupported dict key type {key_ann!r}; only str keys are supported.")
    return {
        "type": "object",
        "additionalProperties": annotation_to_schema(value_ann),
    }


def _schema_for_generic_origin(origin: object, args: tuple[Any, ...]) -> dict[str, Any] | None:
    """Schema for parameterised generics: Literal, Union, list/tuple/dict."""
    if origin is Literal:
        return _schema_for_literal(args)
    if _is_union(origin):
        return {"anyOf": [annotation_to_schema(a) for a in args]}
    if origin in (list, typing.List):  # noqa: UP006
        item_ann = args[0] if args else Any
        return {"type": "array", "items": annotation_to_schema(item_ann)}
    if origin in (tuple, typing.Tuple):  # noqa: UP006
        return _schema_for_tuple(args)
    if origin in (dict, typing.Dict):  # noqa: UP006
        return _schema_for_dict(args)
    return None


def _schema_for_bare_builtin(annotation: object) -> dict[str, Any] | None:
    if annotation is list or annotation is tuple:
        return {"type": "array"}
    if annotation is dict:
        return {"type": "object"}
    return None


def _schema_for_plain_type(annotation: type) -> dict[str, Any] | None:
    scalar = _scalar_schema(annotation)
    if scalar is not None:
        return scalar
    if dataclasses.is_dataclass(annotation):
        return _dataclass_schema(annotation)
    if typing.is_typeddict(annotation):
        return _typeddict_schema(annotation)
    if _is_namedtuple(annotation):
        return _namedtuple_schema(annotation)
    return None


def annotation_to_schema(annotation: object) -> dict[str, Any]:
    """Convert a single type annotation to a JSON Schema fragment.

    Raises :class:`SchemaError` if the annotation is unsupported.
    """
    empty_schema = _schema_for_empty_annotation(annotation)
    if empty_schema is not None:
        return empty_schema

    annotated_schema = _schema_for_annotated(annotation)
    if annotated_schema is not None:
        return annotated_schema

    # Optional[T] / T | None.
    is_opt, inner = _is_optional(annotation)
    if is_opt:
        inner_schema = annotation_to_schema(inner)
        # Compact ``nullable: true`` form - more widely understood by LLMs
        # than the JSON-Schema-draft ``"type": ["X", "null"]`` array form,
        # and less verbose in the tool descriptor.
        new_schema = dict(inner_schema)
        new_schema["nullable"] = True
        return new_schema

    origin = get_origin(annotation)
    args = get_args(annotation)

    generic_schema = _schema_for_generic_origin(origin, args)
    if generic_schema is not None:
        return generic_schema

    bare_schema = _schema_for_bare_builtin(annotation)
    if bare_schema is not None:
        return bare_schema

    if isinstance(annotation, type):
        plain_schema = _schema_for_plain_type(annotation)
        if plain_schema is not None:
            return plain_schema

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
    return bool(isinstance(ann, str) and ann in EXCLUDED_TYPE_NAMES)


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


def _python_type_name(value: object) -> str:
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


def _format_type_decl(type_decl: object) -> str:
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


def _matches_type(value: object, type_decl: object) -> bool:
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


def _validate_anyof(value: object, schema: dict[str, Any], path: str) -> None:
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


def _validate_type_decl(value: object, type_decl: object, path: str) -> None:
    actual_type = _python_type_name(value)
    raise PayloadError(
        f"{path or '<root>'}: expected type {_format_type_decl(type_decl)}, got {actual_type}",
        field=path or None,
        details={
            "field": path or None,
            "expected_type": type_decl,
            "actual_type": actual_type,
        },
    )


def _index_path(path: str, idx: int) -> str:
    return f"{path}[{idx}]" if path else f"[{idx}]"


def _validate_items(value: list[Any], items_schema: dict[str, Any], path: str) -> None:
    for idx, item in enumerate(value):
        _validate_value(item, items_schema, _index_path(path, idx))


def _validate_prefix_items(value: list[Any], prefix_items: list[Any], path: str) -> None:
    for idx, sub_schema in enumerate(prefix_items):
        if idx >= len(value):
            break
        _validate_value(value[idx], sub_schema, _index_path(path, idx))


def _validate_array_value(value: list[Any], schema: dict[str, Any], path: str) -> None:
    items_schema = schema.get("items")
    if isinstance(items_schema, dict):
        _validate_items(value, items_schema, path)
    prefix_items = schema.get("prefixItems")
    if isinstance(prefix_items, list):
        _validate_prefix_items(value, prefix_items, path)


def _is_array_type(type_decl: object, value: object) -> bool:
    if type_decl == "array":
        return True
    return isinstance(type_decl, list) and "array" in type_decl and isinstance(value, list)


def _is_object_type(type_decl: object) -> bool:
    if type_decl == "object":
        return True
    return isinstance(type_decl, list) and "object" in type_decl


def _validate_value(value: object, schema: dict[str, Any], path: str) -> None:
    """Validate ``value`` against ``schema``; raise PayloadError on mismatch."""
    if not schema:
        return  # Any-type, accept everything.

    # nullable: True allows None regardless of the declared ``type``.
    if value is None and schema.get("nullable") is True:
        return

    # anyOf.
    if "anyOf" in schema:
        _validate_anyof(value, schema, path)
        return

    # enum.
    if "enum" in schema and value not in schema["enum"]:
        raise PayloadError(
            f"{path or '<root>'}: value {value!r} not in enum {schema['enum']}",
            field=path or None,
        )

    type_decl = schema.get("type")
    if type_decl is not None and not _matches_type(value, type_decl):
        _validate_type_decl(value, type_decl, path)

    if _is_array_type(type_decl, value) and isinstance(value, list):
        _validate_array_value(value, schema, path)

    if isinstance(value, dict) and _is_object_type(type_decl):
        _validate_object_value(value, schema, path)


def _raise_missing_field(req: str, required: Sequence[str], field_path: str) -> None:
    raise PayloadError(
        f"Missing required field '{field_path}'. Required: {required}.",
        field=field_path,
        details={"missing": req, "required": list(required)},
    )


def _raise_unexpected_field(key: str, expected: list[str], field_path: str) -> None:
    suggestion = _suggest(key, expected)
    msg = f"Unexpected field '{field_path}'. Expected fields: {expected}."
    details: dict[str, Any] = {"unexpected": key, "expected": expected}
    if suggestion is not None:
        msg += f" Did you mean '{suggestion}'?"
        details["did_you_mean"] = suggestion
    raise PayloadError(msg, field=field_path, details=details)


def _check_required_fields(value: dict[str, Any], required: Sequence[str], path: str) -> None:
    for req in required:
        if req not in value:
            req_path = f"{path}.{req}" if path else req
            _raise_missing_field(req, required, req_path)


def _validate_object_value(value: dict[str, Any], schema: dict[str, Any], path: str) -> None:
    properties = schema.get("properties", {})
    required = schema.get("required", [])
    additional = schema.get("additionalProperties", True)
    expected = list(properties.keys())

    _check_required_fields(value, required, path)

    for key, sub_value in value.items():
        sub_path = f"{path}.{key}" if path else key
        if key in properties:
            _validate_value(sub_value, properties[key], sub_path)
        elif additional is False:
            _raise_unexpected_field(key, expected, sub_path)
        elif isinstance(additional, dict):
            _validate_value(sub_value, additional, sub_path)


def validate_payload(payload: dict[str, Any], schema: dict[str, Any]) -> dict[str, Any]:
    """Validate ``payload`` against ``schema`` and return coerced kwargs.

    The schema is expected to be the object schema produced by
    :func:`signature_to_input_schema`.

    Behaviour:

    - Missing required fields raise :class:`PayloadError` with ``field``.
    - Unexpected fields are DROPPED (with a warning) when
      ``additionalProperties`` is ``False`` (the default), instead of failing
      the whole call. LLM-driven callers routinely add stray fields (a
      hallucinated ``max_chars``, a leaked ``additionalProperties``) raises
      :class:`PayloadError` when the schema sets ``additionalProperties: false``,
      with a ``did_you_mean`` hint for a near miss. Required fields and type
      checks still apply.
    - Type mismatches on known fields raise :class:`PayloadError` - no implicit
      string-to-number coercion.
    - Returns the accepted kwargs (unknown fields removed) suitable for
      ``**call``.
    """
    if not isinstance(payload, dict):
        raise PayloadError(
            f"payload must be a dict, got {type(payload).__name__}",
        )

    properties = schema.get("properties", {})
    required = schema.get("required", [])
    additional = schema.get("additionalProperties", True)
    expected = list(properties.keys())

    # Check required fields first so the error is deterministic. A genuinely
    # missing input still fails fast; only unexpected extras are tolerated.
    _check_required_fields(payload, required, "")

    coerced: dict[str, Any] = {}
    for key, value in payload.items():
        if key in properties:
            _validate_value(value, properties[key], key)
            coerced[key] = value
        elif additional is False:
            # Unknown field under a strict schema: reject. Dropping it silently
            # produced a truncated call whose wrong result surfaced far from its
            # cause, and the warning landed in a log nobody reads. Rejecting puts
            # the error where the mistake is, and `did_you_mean` makes a typo
            # cheap to fix.
            _raise_unexpected_field(key, expected, key)
        else:
            # `additionalProperties` allowed: keep the extra field as-is.
            coerced[key] = value

    return coerced
