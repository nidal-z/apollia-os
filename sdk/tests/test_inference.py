"""Tests for signature inference + payload validation."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Literal, NamedTuple, Optional, TypedDict, Union

import pytest

from apollia._internal.inference import (
    annotation_to_schema,
    return_to_output_schema,
    signature_to_input_schema,
    validate_payload,
)
from apollia.errors import PayloadError, SchemaError


# Module-level helpers (need to be resolvable by ``get_type_hints`` under
# ``from __future__ import annotations``).
class Ctx:  # noqa: N801 — name is the exclusion key
    """Stand-in for the runtime ``Ctx`` protocol — recognised by name."""


@dataclass
class _ModuleCfg:
    a: int
    b: str = "x"


# ────────────────────── annotation_to_schema ──────────────────────


def test_annotation_str() -> None:
    assert annotation_to_schema(str) == {"type": "string"}


def test_annotation_int() -> None:
    assert annotation_to_schema(int) == {"type": "integer"}


def test_annotation_float() -> None:
    assert annotation_to_schema(float) == {"type": "number"}


def test_annotation_bool() -> None:
    assert annotation_to_schema(bool) == {"type": "boolean"}


def test_annotation_bytes() -> None:
    assert annotation_to_schema(bytes) == {"type": "string", "format": "byte"}


def test_annotation_any() -> None:
    assert annotation_to_schema(Any) == {}


def test_annotation_optional_str() -> None:
    schema = annotation_to_schema(Optional[str])
    assert schema["type"] == ["string", "null"]


def test_annotation_pep604_optional() -> None:
    schema = annotation_to_schema(str | None)
    assert schema["type"] == ["string", "null"]


def test_annotation_union() -> None:
    schema = annotation_to_schema(Union[str, int])
    assert "anyOf" in schema
    assert {"type": "string"} in schema["anyOf"]
    assert {"type": "integer"} in schema["anyOf"]


def test_annotation_pep604_union() -> None:
    schema = annotation_to_schema(str | int)
    assert "anyOf" in schema


def test_annotation_literal_strings() -> None:
    schema = annotation_to_schema(Literal["a", "b"])
    assert schema == {"enum": ["a", "b"], "type": "string"}


def test_annotation_literal_ints() -> None:
    schema = annotation_to_schema(Literal[1, 2, 3])
    assert schema["type"] == "integer"
    assert schema["enum"] == [1, 2, 3]


def test_annotation_list_of_str() -> None:
    assert annotation_to_schema(list[str]) == {
        "type": "array",
        "items": {"type": "string"},
    }


def test_annotation_list_no_args() -> None:
    schema = annotation_to_schema(list)
    # Without typed parameters, list becomes "array" with any items.
    assert schema["type"] == "array"


def test_annotation_dict_str_int() -> None:
    assert annotation_to_schema(dict[str, int]) == {
        "type": "object",
        "additionalProperties": {"type": "integer"},
    }


def test_annotation_dict_unsupported_key() -> None:
    with pytest.raises(SchemaError):
        annotation_to_schema(dict[int, int])


def test_annotation_tuple_homogeneous() -> None:
    schema = annotation_to_schema(tuple[int, ...])
    assert schema == {"type": "array", "items": {"type": "integer"}}


def test_annotation_tuple_fixed_length() -> None:
    schema = annotation_to_schema(tuple[int, str])
    assert schema["type"] == "array"
    assert schema["minItems"] == 2
    assert schema["maxItems"] == 2


def test_annotation_unsupported() -> None:
    class Foo:
        pass

    with pytest.raises(SchemaError):
        annotation_to_schema(Foo)


def test_annotation_dataclass() -> None:
    @dataclass
    class Foo:
        a: int
        b: str = "x"

    schema = annotation_to_schema(Foo)
    assert schema["type"] == "object"
    assert schema["properties"]["a"]["type"] == "integer"
    assert schema["properties"]["b"]["type"] == "string"
    assert schema["required"] == ["a"]


def test_annotation_typed_dict() -> None:
    class Foo(TypedDict):
        a: int
        b: str

    schema = annotation_to_schema(Foo)
    assert schema["type"] == "object"
    assert set(schema["required"]) == {"a", "b"}


def test_annotation_namedtuple() -> None:
    class P(NamedTuple):
        x: int
        y: int = 0

    schema = annotation_to_schema(P)
    assert schema["properties"]["x"]["type"] == "integer"
    assert "x" in schema["required"]
    assert "y" not in schema["required"]


# ────────────────────── signature_to_input_schema ──────────────────────


def test_signature_str_required() -> None:
    def fn(path: str) -> None: ...

    schema = signature_to_input_schema(fn)
    assert schema == {
        "type": "object",
        "properties": {"path": {"type": "string"}},
        "required": ["path"],
        "additionalProperties": False,
    }


def test_signature_with_default() -> None:
    def fn(count: int = 10) -> None: ...

    schema = signature_to_input_schema(fn)
    assert "required" not in schema or "count" not in schema["required"]
    assert schema["properties"]["count"]["type"] == "integer"


def test_signature_optional_not_required() -> None:
    def fn(name: str | None = None) -> None: ...

    schema = signature_to_input_schema(fn)
    assert "required" not in schema or "name" not in schema["required"]


def test_signature_optional_without_default_not_required() -> None:
    def fn(name: str | None) -> None: ...

    schema = signature_to_input_schema(fn)
    # Optional → never required, even without a default.
    assert "required" not in schema or "name" not in schema["required"]


def test_signature_excludes_self_and_ctx() -> None:
    def fn(self: Any, path: str, ctx: Any) -> None: ...

    schema = signature_to_input_schema(fn)
    assert set(schema["properties"].keys()) == {"path"}


def test_signature_excludes_cls() -> None:
    def fn(cls: Any, x: int) -> None: ...

    schema = signature_to_input_schema(fn)
    assert set(schema["properties"].keys()) == {"x"}


def test_signature_excludes_ctx_type_annotation() -> None:
    def fn(path: str, my_ctx: Ctx) -> None: ...

    schema = signature_to_input_schema(fn)
    # my_ctx is annotated as the Ctx class → excluded.
    assert "my_ctx" not in schema["properties"]
    assert "path" in schema["properties"]


def test_signature_var_args_excluded() -> None:
    def fn(path: str, *args: int, **kwargs: int) -> None: ...

    schema = signature_to_input_schema(fn)
    assert set(schema["properties"].keys()) == {"path"}


def test_signature_unsupported_raises() -> None:
    class Foo:
        pass

    def fn(x: Foo) -> None: ...

    with pytest.raises(SchemaError):
        signature_to_input_schema(fn)


# ────────────────────── return_to_output_schema ──────────────────────


def test_return_missing_annotation() -> None:
    def fn(): ...  # type: ignore[no-untyped-def]

    assert return_to_output_schema(fn) == {}


def test_return_none() -> None:
    def fn() -> None: ...

    assert return_to_output_schema(fn) == {}


def test_return_str() -> None:
    def fn() -> str: ...

    assert return_to_output_schema(fn) == {"type": "string"}


def test_return_unsupported_falls_back() -> None:
    class Foo:
        pass

    def fn() -> Foo: ...

    assert return_to_output_schema(fn) == {}


# ────────────────────── validate_payload ──────────────────────


def _schema_path_required() -> dict[str, Any]:
    def fn(path: str) -> None: ...

    return signature_to_input_schema(fn)


def test_validate_happy() -> None:
    schema = _schema_path_required()
    kwargs = validate_payload({"path": "a.pdf"}, schema)
    assert kwargs == {"path": "a.pdf"}


def test_validate_missing_required() -> None:
    schema = _schema_path_required()
    with pytest.raises(PayloadError) as exc:
        validate_payload({}, schema)
    assert exc.value.field == "path"


def test_validate_type_mismatch() -> None:
    schema = _schema_path_required()
    with pytest.raises(PayloadError) as exc:
        validate_payload({"path": 42}, schema)
    assert exc.value.field == "path"


def test_validate_extra_field_rejected() -> None:
    schema = _schema_path_required()
    with pytest.raises(PayloadError) as exc:
        validate_payload({"path": "x", "extra": "no"}, schema)
    assert exc.value.field == "extra"


def test_validate_payload_not_dict() -> None:
    with pytest.raises(PayloadError):
        validate_payload([], {"type": "object"})  # type: ignore[arg-type]


def test_validate_with_optional() -> None:
    def fn(path: str, count: int | None = None) -> None: ...

    schema = signature_to_input_schema(fn)
    # Optional accepts None.
    assert validate_payload({"path": "a", "count": None}, schema) == {
        "path": "a",
        "count": None,
    }
    # And an integer.
    assert validate_payload({"path": "a", "count": 7}, schema) == {
        "path": "a",
        "count": 7,
    }
    # And accepts being absent.
    assert validate_payload({"path": "a"}, schema) == {"path": "a"}


def test_validate_with_literal_rejects_other_value() -> None:
    def fn(mode: Literal["fast", "slow"]) -> None: ...

    schema = signature_to_input_schema(fn)
    with pytest.raises(PayloadError):
        validate_payload({"mode": "medium"}, schema)
    assert validate_payload({"mode": "fast"}, schema) == {"mode": "fast"}


def test_validate_list_of_str() -> None:
    def fn(items: list[str]) -> None: ...

    schema = signature_to_input_schema(fn)
    assert validate_payload({"items": ["a", "b"]}, schema) == {"items": ["a", "b"]}
    with pytest.raises(PayloadError):
        validate_payload({"items": ["a", 2]}, schema)


def test_validate_dict_additional_properties_value_schema() -> None:
    def fn(counts: dict[str, int]) -> None: ...

    schema = signature_to_input_schema(fn)
    assert validate_payload({"counts": {"a": 1}}, schema) == {"counts": {"a": 1}}
    with pytest.raises(PayloadError):
        validate_payload({"counts": {"a": "not-int"}}, schema)


def test_validate_bool_is_not_int() -> None:
    def fn(count: int) -> None: ...

    schema = signature_to_input_schema(fn)
    with pytest.raises(PayloadError):
        validate_payload({"count": True}, schema)


def test_validate_union_accepts_either() -> None:
    def fn(value: str | int) -> None: ...

    schema = signature_to_input_schema(fn)
    assert validate_payload({"value": "x"}, schema) == {"value": "x"}
    assert validate_payload({"value": 42}, schema) == {"value": 42}
    with pytest.raises(PayloadError):
        validate_payload({"value": 1.5}, schema)


def test_validate_dataclass_payload() -> None:
    def fn(cfg: _ModuleCfg) -> None: ...

    schema = signature_to_input_schema(fn)
    assert validate_payload({"cfg": {"a": 1}}, schema) == {"cfg": {"a": 1}}
    with pytest.raises(PayloadError):
        validate_payload({"cfg": {}}, schema)


# Eliminate unused field/return import warnings.
_ = field
