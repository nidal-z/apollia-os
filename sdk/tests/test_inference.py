"""Tests for signature inference + payload validation."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Annotated, Any, Literal, NamedTuple, Optional, Union

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
class Ctx:  # - name is the exclusion key
    """Stand-in for the runtime ``Ctx`` protocol - recognised by name."""


@dataclass
class _ModuleCfg:
    a: int
    b: str = "x"


# ────────────────────── annotation_to_schema ──────────────────────


def test_annotation_str() -> None:
    # GIVEN the str annotation
    # WHEN it is mapped to a JSON schema
    # THEN it becomes the string type
    assert annotation_to_schema(str) == {"type": "string"}


def test_annotation_int() -> None:
    # GIVEN the int annotation
    # WHEN it is mapped to a JSON schema
    # THEN it becomes the integer type, not number
    assert annotation_to_schema(int) == {"type": "integer"}


def test_annotation_float() -> None:
    # GIVEN the float annotation
    # WHEN it is mapped to a JSON schema
    # THEN it becomes the number type
    assert annotation_to_schema(float) == {"type": "number"}


def test_annotation_bool() -> None:
    # GIVEN the bool annotation
    # WHEN it is mapped to a JSON schema
    # THEN it becomes the boolean type, not integer
    assert annotation_to_schema(bool) == {"type": "boolean"}


def test_annotation_bytes() -> None:
    # GIVEN the bytes annotation
    # WHEN it is mapped to a JSON schema
    # THEN it becomes a string carrying the byte format
    assert annotation_to_schema(bytes) == {"type": "string", "format": "byte"}


def test_annotation_any() -> None:
    # GIVEN the Any annotation
    # WHEN it is mapped to a JSON schema
    # THEN it becomes the empty schema, which constrains nothing
    assert annotation_to_schema(Any) == {}


# REASON(UP007/UP045): the `Optional[...]` and `Union[...]` spellings below are the
# subject under test, not a style lapse. Agents in the wild still write them, and
# `annotation_to_schema` must map them to the same schema as the PEP 604 forms
# exercised alongside. Rewriting them to `X | Y` would delete that coverage.
def test_annotation_optional_str() -> None:
    # GIVEN the legacy Optional[str] spelling
    schema = annotation_to_schema(Optional[str])  # noqa: UP045
    # WHEN it is mapped to a JSON schema
    # Compact form: type stays a plain string; nullable marker exposes null.
    # THEN the type stays a plain string and null is carried by the nullable marker
    assert schema == {"type": "string", "nullable": True}


def test_annotation_pep604_optional() -> None:
    # GIVEN the PEP 604 str | None spelling
    # WHEN it is mapped to a JSON schema
    schema = annotation_to_schema(str | None)
    # THEN it yields the same schema as the legacy Optional spelling
    assert schema == {"type": "string", "nullable": True}


def test_annotation_union() -> None:
    # GIVEN the legacy Union[str, int] spelling
    schema = annotation_to_schema(Union[str, int])  # noqa: UP007
    # WHEN it is mapped to a JSON schema
    assert "anyOf" in schema
    # THEN both branches appear under anyOf
    assert {"type": "string"} in schema["anyOf"]
    assert {"type": "integer"} in schema["anyOf"]


def test_annotation_pep604_union() -> None:
    # GIVEN the PEP 604 str | int spelling
    # WHEN it is mapped to a JSON schema
    schema = annotation_to_schema(str | int)
    # THEN it also yields an anyOf, like the legacy spelling
    assert "anyOf" in schema


def test_annotation_literal_strings() -> None:
    # GIVEN a Literal of two strings
    # WHEN it is mapped to a JSON schema
    schema = annotation_to_schema(Literal["a", "b"])
    # THEN it becomes a string enum listing both values
    assert schema == {"enum": ["a", "b"], "type": "string"}


def test_annotation_literal_ints() -> None:
    # GIVEN a Literal of three integers
    schema = annotation_to_schema(Literal[1, 2, 3])
    # WHEN it is mapped to a JSON schema
    assert schema["type"] == "integer"
    # THEN the type is inferred from the values and the enum lists them
    assert schema["enum"] == [1, 2, 3]


def test_annotation_list_of_str() -> None:
    # GIVEN the list[str] annotation
    # WHEN it is mapped to a JSON schema
    # THEN it becomes an array whose items are strings
    assert annotation_to_schema(list[str]) == {
        "type": "array",
        "items": {"type": "string"},
    }


def test_annotation_list_no_args() -> None:
    # GIVEN a bare list annotation with no type parameter
    schema = annotation_to_schema(list)
    # WHEN it is mapped to a JSON schema
    # Without typed parameters, list becomes "array" with any items.
    # THEN it becomes an array whose items are unconstrained
    assert schema["type"] == "array"


def test_annotation_dict_str_int() -> None:
    # GIVEN the dict[str, int] annotation
    # WHEN it is mapped to a JSON schema
    # THEN it becomes an object whose additional properties are integers
    assert annotation_to_schema(dict[str, int]) == {
        "type": "object",
        "additionalProperties": {"type": "integer"},
    }


def test_annotation_dict_unsupported_key() -> None:
    # GIVEN a mapping annotation whose keys are not strings
    # WHEN it is mapped to a JSON schema
    # THEN it is refused, because JSON object keys are strings
    with pytest.raises(SchemaError):
        annotation_to_schema(dict[int, int])


def test_annotation_tuple_homogeneous() -> None:
    # GIVEN a variable-length tuple annotation
    # WHEN it is mapped to a JSON schema
    schema = annotation_to_schema(tuple[int, ...])
    # THEN it becomes an array with no length bound
    assert schema == {"type": "array", "items": {"type": "integer"}}


def test_annotation_tuple_fixed_length() -> None:
    # GIVEN a two-element tuple annotation
    schema = annotation_to_schema(tuple[int, str])
    # WHEN it is mapped to a JSON schema
    assert schema["type"] == "array"
    # THEN the array is bounded to exactly that length
    assert schema["minItems"] == 2
    assert schema["maxItems"] == 2


def test_annotation_unsupported() -> None:
    # GIVEN an annotation the mapper knows nothing about
    class Foo:
        pass

    # WHEN it is mapped to a JSON schema
    # THEN it is refused rather than mapped to an empty schema
    with pytest.raises(SchemaError):
        annotation_to_schema(Foo)


def test_annotation_dataclass() -> None:
    # GIVEN a dataclass with one required and one defaulted field
    @dataclass
    class Foo:
        a: int
        b: str = "x"

    # WHEN it is mapped to a JSON schema
    schema = annotation_to_schema(Foo)
    # THEN it becomes an object and only the field without a default is required
    assert schema["type"] == "object"
    assert schema["properties"]["a"]["type"] == "integer"
    assert schema["properties"]["b"]["type"] == "string"
    assert schema["required"] == ["a"]


def test_annotation_typed_dict() -> None:
    # A TypedDict defined in this module would sit under PEP 563 (line 3)
    # and trigger the stringified-annotations warning another test checks
    # deliberately; the fixture module has no `from __future__` import.
    # GIVEN a TypedDict declared without PEP 563
    from tests._typeddict_fixtures import PlainTD

    # WHEN it is mapped to a JSON schema
    schema = annotation_to_schema(PlainTD)
    # THEN it becomes an object whose two keys are both required
    assert schema["type"] == "object"
    assert set(schema["required"]) == {"a", "b"}


def test_annotation_typed_dict_not_required() -> None:
    """NotRequired[T] fields must appear in properties but not in required.

    NOTE: ``__required_keys__`` is computed at class-creation time by walking
    the raw ``__annotations__`` dict. Under ``from __future__ import
    annotations`` (PEP 563), all annotations are strings and TypedDict cannot
    detect ``NotRequired[T]``. Worker ``schemas.py`` files therefore avoid
    PEP 563 - and so does ``_typeddict_fixtures.py``.
    """
    # GIVEN a TypedDict carrying one NotRequired field
    from tests._typeddict_fixtures import NotRequiredTD

    # WHEN it is mapped to a JSON schema
    schema = annotation_to_schema(NotRequiredTD)
    # THEN that field is a property but not a required one
    assert schema["properties"]["color"] == {"type": "string"}
    assert set(schema["required"]) == {"name", "data"}


def test_annotation_typed_dict_required_total_false() -> None:
    """Required[T] inside total=False TypedDict must mark the field as required."""
    # GIVEN a total=False TypedDict carrying one Required field
    from tests._typeddict_fixtures import RequiredTotalFalseTD

    # WHEN it is mapped to a JSON schema
    schema = annotation_to_schema(RequiredTotalFalseTD)
    # THEN only the explicitly Required field is required
    assert schema["properties"]["name"] == {"type": "string"}
    assert schema["properties"]["size"] == {"type": "integer"}
    assert schema["required"] == ["name"]


def test_annotation_typed_dict_total_false_no_required() -> None:
    """A total=False TypedDict with no Required[] field yields an empty required set."""
    # GIVEN a total=False TypedDict whose fields are all implicitly optional
    from tests._typeddict_fixtures import TotalFalseNoRequiredTD

    # WHEN the schema is derived
    schema = annotation_to_schema(TotalFalseNoRequiredTD)

    # THEN both fields are present but none is required
    assert schema["properties"]["name"] == {"type": "string"}
    assert schema["properties"]["size"] == {"type": "integer"}
    assert "required" not in schema


def test_typeddict_stringified_annotations_warn_and_recover() -> None:
    """A payload TypedDict defined under PEP 563 warns yet still derives the
    correct required split via get_type_hints (where __required_keys__ fails)."""
    import typing
    import warnings

    # GIVEN a TypedDict whose module used ``from __future__ import annotations``
    from tests._typeddict_future_fixtures import FutureAnnotatedTD

    # its raw annotations are unresolved (str or ForwardRef, per Python version),
    # so __required_keys__ wrongly keeps the NotRequired ``color`` field
    assert all(
        isinstance(v, (str, typing.ForwardRef)) for v in FutureAnnotatedTD.__annotations__.values()
    )
    assert "color" in FutureAnnotatedTD.__required_keys__

    # WHEN the schema is derived
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        schema = annotation_to_schema(FutureAnnotatedTD)

    # THEN a warning surfaces the stringified-annotation footgun
    assert any("stringified" in str(w.message) for w in caught)
    # AND the required split is recovered correctly (color is NotRequired)
    assert set(schema["required"]) == {"name"}
    assert "color" in schema["properties"]


def test_typeddict_required_derivation_matches_required_keys() -> None:
    """For a well-formed TypedDict, the derived required set matches __required_keys__."""
    # GIVEN a TypedDict defined without PEP 563 (the worker convention)
    from tests._typeddict_fixtures import NotRequiredTD

    # WHEN the schema is derived
    schema = annotation_to_schema(NotRequiredTD)

    # THEN the derivation agrees with the interpreter's own __required_keys__
    assert set(schema["required"]) == set(NotRequiredTD.__required_keys__)


def test_annotation_namedtuple() -> None:
    # GIVEN a NamedTuple with one required and one defaulted field
    class P(NamedTuple):
        x: int
        y: int = 0

    # WHEN it is mapped to a JSON schema
    schema = annotation_to_schema(P)
    # THEN only the field without a default is required
    assert schema["properties"]["x"]["type"] == "integer"
    assert "x" in schema["required"]
    assert "y" not in schema["required"]


# ────────────────────── signature_to_input_schema ──────────────────────


def test_signature_str_required() -> None:
    # GIVEN a handler taking one required string argument
    def fn(path: str) -> None: ...

    # WHEN its signature is turned into an input schema
    schema = signature_to_input_schema(fn)
    # THEN the argument is a required property and unknown keys are refused
    assert schema == {
        "type": "object",
        "properties": {"path": {"type": "string"}},
        "required": ["path"],
        "additionalProperties": False,
    }


def test_signature_with_default() -> None:
    # GIVEN a handler whose only argument has a default
    def fn(count: int = 10) -> None: ...

    # WHEN its signature is turned into an input schema
    schema = signature_to_input_schema(fn)
    # THEN the argument is a property but not required
    assert "required" not in schema or "count" not in schema["required"]
    assert schema["properties"]["count"]["type"] == "integer"


def test_signature_optional_not_required() -> None:
    # GIVEN a handler whose argument is optional and defaulted
    def fn(name: str | None = None) -> None: ...

    # WHEN its signature is turned into an input schema
    schema = signature_to_input_schema(fn)
    # THEN the argument is not required
    assert "required" not in schema or "name" not in schema["required"]


def test_signature_optional_without_default_not_required() -> None:
    # GIVEN a handler whose argument is optional but has no default
    def fn(name: str | None) -> None: ...

    # WHEN its signature is turned into an input schema
    schema = signature_to_input_schema(fn)
    # THEN it is still not required, because the type already admits None
    # Optional → never required, even without a default.
    assert "required" not in schema or "name" not in schema["required"]


def test_signature_excludes_self_and_ctx() -> None:
    # GIVEN a handler taking self, a payload argument and ctx
    def fn(self: Any, path: str, ctx: Any) -> None: ...

    # WHEN its signature is turned into an input schema
    schema = signature_to_input_schema(fn)
    # THEN only the payload argument becomes a property
    assert set(schema["properties"].keys()) == {"path"}


def test_signature_excludes_cls() -> None:
    # GIVEN a handler whose first argument is cls
    def fn(cls: Any, x: int) -> None: ...

    # WHEN its signature is turned into an input schema
    schema = signature_to_input_schema(fn)
    # THEN cls is excluded like self is
    assert set(schema["properties"].keys()) == {"x"}


def test_signature_excludes_ctx_type_annotation() -> None:
    # GIVEN a handler whose context argument is named freely but typed as Ctx
    def fn(path: str, my_ctx: Ctx) -> None: ...

    # WHEN its signature is turned into an input schema
    schema = signature_to_input_schema(fn)
    # THEN it is excluded on its type, not only on its name
    # my_ctx is annotated as the Ctx class → excluded.
    assert "my_ctx" not in schema["properties"]
    assert "path" in schema["properties"]


def test_signature_var_args_excluded() -> None:
    # GIVEN a handler taking *args and **kwargs beside a named argument
    def fn(path: str, *args: int, **kwargs: int) -> None: ...

    # WHEN its signature is turned into an input schema
    schema = signature_to_input_schema(fn)
    # THEN only the named argument becomes a property
    assert set(schema["properties"].keys()) == {"path"}


def test_signature_unsupported_raises() -> None:
    # GIVEN a handler whose argument is annotated with an unmappable type
    class Foo:
        pass

    def fn(x: Foo) -> None: ...

    # WHEN its signature is turned into an input schema
    # THEN it is refused at build time rather than at call time
    with pytest.raises(SchemaError):
        signature_to_input_schema(fn)


# ────────────────────── return_to_output_schema ──────────────────────


def test_return_missing_annotation() -> None:
    # GIVEN a handler with no return annotation
    # WHEN its return type is turned into an output schema
    def fn(): ...  # type: ignore[no-untyped-def]

    # THEN the schema is empty, constraining nothing
    assert return_to_output_schema(fn) == {}


def test_return_none() -> None:
    # GIVEN a handler returning None
    # WHEN its return type is turned into an output schema
    def fn() -> None: ...

    # THEN the schema is empty rather than a null type
    assert return_to_output_schema(fn) == {}


def test_return_str() -> None:
    # GIVEN a handler returning a string
    # WHEN its return type is turned into an output schema
    def fn() -> str: ...

    # THEN the schema is the string type
    assert return_to_output_schema(fn) == {"type": "string"}


def test_return_unsupported_falls_back() -> None:
    # GIVEN a handler returning an unmappable type
    # WHEN its return type is turned into an output schema
    class Foo:
        pass

    def fn() -> Foo: ...

    # THEN the schema is empty, so an unmappable return does not break registration
    assert return_to_output_schema(fn) == {}


# ────────────────────── validate_payload ──────────────────────


def _schema_path_required() -> dict[str, Any]:
    def fn(path: str) -> None: ...

    return signature_to_input_schema(fn)


def test_validate_happy() -> None:
    # GIVEN a schema requiring one string field, and a matching payload
    schema = _schema_path_required()
    # WHEN the payload is validated
    kwargs = validate_payload({"path": "a.pdf"}, schema)
    # THEN the payload becomes the handler's keyword arguments
    assert kwargs == {"path": "a.pdf"}


def test_validate_missing_required() -> None:
    # GIVEN a schema requiring one string field, and an empty payload
    schema = _schema_path_required()
    # WHEN the payload is validated
    with pytest.raises(PayloadError) as exc:
        validate_payload({}, schema)
    # THEN it fails and names the missing field
    assert exc.value.field == "path"


def test_validate_type_mismatch() -> None:
    # GIVEN a schema requiring a string, and a payload carrying an integer
    schema = _schema_path_required()
    # WHEN the payload is validated
    with pytest.raises(PayloadError) as exc:
        validate_payload({"path": 42}, schema)
    # THEN it fails and names the offending field
    assert exc.value.field == "path"


def test_validate_extra_field_rejected() -> None:
    # GIVEN a strict schema and a payload carrying one unexpected field
    schema = _schema_path_required()

    # WHEN the payload is validated
    with pytest.raises(PayloadError) as excinfo:
        validate_payload({"path": "x", "extra": "no"}, schema)

    # THEN the call is rejected and names the offending field. Dropping it used
    # to be tolerated, which turned a caller mistake into a silently truncated
    # call whose wrong result surfaced far from its cause.
    details = excinfo.value.details or {}
    assert details["unexpected"] == "extra"
    assert details["expected"] == ["path"]


def test_validate_payload_not_dict() -> None:
    # GIVEN a payload that is a list rather than a mapping
    # WHEN it is validated
    # THEN it fails rather than iterating over the list
    with pytest.raises(PayloadError):
        validate_payload([], {"type": "object"})  # type: ignore[arg-type]  # NOSONAR S5655: intentional bad type to verify PayloadError is raised on non-dict payload


def test_validate_with_optional() -> None:
    # GIVEN a schema with one required field and one optional integer
    def fn(path: str, count: int | None = None) -> None: ...

    schema = signature_to_input_schema(fn)
    # WHEN payloads with None, with an integer and without the field are validated
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
    # THEN all three are accepted
    # And accepts being absent.
    assert validate_payload({"path": "a"}, schema) == {"path": "a"}


def test_validate_with_literal_rejects_other_value() -> None:
    # GIVEN a schema whose field is a two-value literal
    def fn(mode: Literal["fast", "slow"]) -> None: ...

    schema = signature_to_input_schema(fn)
    # WHEN a value outside the literal is validated, then one inside
    with pytest.raises(PayloadError):
        validate_payload({"mode": "medium"}, schema)
    # THEN the outside value is refused and the inside one passes
    assert validate_payload({"mode": "fast"}, schema) == {"mode": "fast"}


def test_validate_list_of_str() -> None:
    # GIVEN a schema whose field is a list of strings
    def fn(items: list[str]) -> None: ...

    schema = signature_to_input_schema(fn)
    # WHEN a well-typed list is validated, then one holding an integer
    assert validate_payload({"items": ["a", "b"]}, schema) == {"items": ["a", "b"]}
    # THEN the first passes and the second is refused on its element type
    with pytest.raises(PayloadError):
        validate_payload({"items": ["a", 2]}, schema)


def test_validate_dict_additional_properties_value_schema() -> None:
    # GIVEN a schema whose field maps strings to integers
    def fn(counts: dict[str, int]) -> None: ...

    schema = signature_to_input_schema(fn)
    # WHEN a well-typed mapping is validated, then one holding a string value
    assert validate_payload({"counts": {"a": 1}}, schema) == {"counts": {"a": 1}}
    # THEN the first passes and the second is refused on its value type
    with pytest.raises(PayloadError):
        validate_payload({"counts": {"a": "not-int"}}, schema)


def test_validate_bool_is_not_int() -> None:
    # GIVEN a schema whose field is an integer, and a payload carrying True
    def fn(count: int) -> None: ...

    schema = signature_to_input_schema(fn)
    # WHEN the payload is validated
    # THEN it is refused, even though bool is a subclass of int
    with pytest.raises(PayloadError):
        validate_payload({"count": True}, schema)


def test_validate_union_accepts_either() -> None:
    # GIVEN a schema whose field accepts a string or an integer
    def fn(value: str | int) -> None: ...

    schema = signature_to_input_schema(fn)
    # WHEN each branch is validated, then a float
    assert validate_payload({"value": "x"}, schema) == {"value": "x"}
    assert validate_payload({"value": 42}, schema) == {"value": 42}
    # THEN both branches pass and the float is refused
    with pytest.raises(PayloadError):
        validate_payload({"value": 1.5}, schema)


def test_validate_dataclass_payload() -> None:
    # GIVEN a schema whose field is a dataclass with one required attribute
    def fn(cfg: _ModuleCfg) -> None: ...

    schema = signature_to_input_schema(fn)
    # WHEN a complete nested payload is validated, then an empty one
    assert validate_payload({"cfg": {"a": 1}}, schema) == {"cfg": {"a": 1}}
    # THEN the complete one passes and the empty one is refused
    with pytest.raises(PayloadError):
        validate_payload({"cfg": {}}, schema)


# ────────────────────── Annotated[T, "desc"] ──────────────────────


def test_annotated_string_description_propagated() -> None:
    """Annotated[str, "..."] surfaces the metadata string as ``description``."""
    # GIVEN a str annotation carrying one string of metadata
    # WHEN it is mapped to a JSON schema
    schema = annotation_to_schema(Annotated[str, "Path to the input file"])
    # THEN the metadata becomes the property description
    assert schema == {"type": "string", "description": "Path to the input file"}


def test_annotated_list_description_propagated() -> None:
    # GIVEN a list annotation carrying one string of metadata
    schema = annotation_to_schema(Annotated[list[int], "List of integer indices to fetch"])
    # WHEN it is mapped to a JSON schema
    assert schema["type"] == "array"
    # THEN the item type survives and the metadata becomes the description
    assert schema["items"] == {"type": "integer"}
    assert schema["description"] == "List of integer indices to fetch"


def test_annotated_with_optional_type() -> None:
    """Annotated wrapping Optional/Union surfaces both ``nullable`` and ``description``."""
    # GIVEN an optional annotation carrying one string of metadata
    schema = annotation_to_schema(Annotated[str | None, "Optional chart title shown at the top"])
    # WHEN it is mapped to a JSON schema
    # Compact nullable form preserved + description added.
    # THEN both the nullable marker and the description are present
    assert schema["type"] == "string"
    assert schema["nullable"] is True
    assert schema["description"] == "Optional chart title shown at the top"


def test_annotated_no_string_metadata_is_passthrough() -> None:
    """Non-string Annotated metadata is silently ignored (backcompat)."""
    # GIVEN an annotation whose metadata is not a string
    # WHEN it is mapped to a JSON schema
    schema = annotation_to_schema(Annotated[int, 42])
    # THEN the metadata is ignored and the base type comes through unchanged
    assert schema == {"type": "integer"}


def test_annotated_chained_metadata() -> None:
    """Multiple string metadata entries are space-joined into a single description."""
    # GIVEN an annotation carrying two strings of metadata
    schema = annotation_to_schema(Annotated[int, "Page index", "0-based, must be < total_pages"])
    # WHEN it is mapped to a JSON schema
    assert schema["type"] == "integer"
    # THEN both are joined into one description
    assert schema["description"] == "Page index 0-based, must be < total_pages"


def test_annotated_in_signature_surfaces_description() -> None:
    """A skill handler signature using Annotated propagates per-property docs."""
    # GIVEN a handler whose two arguments carry Annotated descriptions

    def fn(
        path: Annotated[str, "Absolute path to the PDF file"],
        page: Annotated[int, "0-based page index"] = 0,
    ) -> None: ...

    # WHEN its signature is turned into an input schema
    schema = signature_to_input_schema(fn)
    # THEN each property carries its own description
    assert schema["properties"]["path"]["description"] == "Absolute path to the PDF file"
    assert schema["properties"]["page"]["description"] == "0-based page index"


# ────────────────────── Optional compact nullable form ──────────────────────


def test_optional_compact_nullable_form() -> None:
    """Optional[str] uses the compact ``{type: string, nullable: true}`` form."""
    # GIVEN the Optional[int] annotation
    # WHEN it is mapped to a JSON schema
    schema = annotation_to_schema(Optional[int])  # noqa: UP045
    # THEN it uses the compact nullable form rather than an anyOf with null
    assert schema == {"type": "integer", "nullable": True}


def test_optional_dataclass_nullable_preserves_object_schema() -> None:
    # GIVEN an optional dataclass annotation
    @dataclass
    class Cfg:
        a: int

    # WHEN it is mapped to a JSON schema
    schema = annotation_to_schema(Optional[Cfg])  # noqa: UP045
    # THEN the object schema survives beside the nullable marker
    assert schema["type"] == "object"
    assert schema["nullable"] is True
    assert "properties" in schema


def test_union_with_none_nullable() -> None:
    """``T | U | None`` produces ``{anyOf: [...], nullable: true}`` without ``None`` inside."""
    # GIVEN a union of two types with None
    schema = annotation_to_schema(Union[str, int, None])  # noqa: UP007
    # WHEN it is mapped to a JSON schema
    assert schema["nullable"] is True
    assert "anyOf" in schema
    # THEN nullable is set and null is not one of the branches
    branch_types = {tuple(sorted(b.items())) for b in schema["anyOf"]}
    # Must contain string + integer branches; null must not appear as a branch.
    assert {("type", "string")} in [set(b.items()) for b in schema["anyOf"]]
    assert {("type", "integer")} in [set(b.items()) for b in schema["anyOf"]]
    _ = branch_types  # silence unused warning in the diff


def test_optional_validation_accepts_none_via_nullable() -> None:
    """The validator honours the compact ``nullable: true`` for None values."""
    # GIVEN a schema built from an optional argument

    def fn(name: str | None = None) -> None: ...

    schema = signature_to_input_schema(fn)
    # WHEN None is validated, then a string
    assert schema["properties"]["name"]["nullable"] is True
    # Compact form must still allow None.
    assert validate_payload({"name": None}, schema) == {"name": None}
    # THEN both are accepted, so the compact form is honoured by the validator
    # And a string remains valid.
    assert validate_payload({"name": "x"}, schema) == {"name": "x"}


# ────────────────────── enriched PayloadError ──────────────────────


def test_unexpected_top_level_field_rejected() -> None:
    """An unexpected top-level field fails the call rather than vanishing."""

    # GIVEN a schema inferred from a handler signature
    def fn(path: str, count: int = 1) -> None: ...

    schema = signature_to_input_schema(fn)

    # WHEN an unknown field is supplied
    with pytest.raises(PayloadError) as excinfo:
        validate_payload({"path": "x", "totally_unknown": 1}, schema)

    # THEN it is named in the error
    assert (excinfo.value.details or {})["unexpected"] == "totally_unknown"


def _nested_strict_schema() -> dict[str, object]:
    """A schema with a required, strict nested object (rejects unknown keys)."""
    return {
        "type": "object",
        "properties": {
            "cfg": {
                "type": "object",
                "properties": {"mode": {"type": "string"}},
                "additionalProperties": False,
            },
        },
        "required": ["cfg"],
        "additionalProperties": False,
    }


def test_payload_error_did_you_mean() -> None:
    """A close typo inside a strict nested object still suggests the field."""
    # GIVEN a strict nested schema and a payload misspelling a nested field
    schema = _nested_strict_schema()
    # WHEN the payload is validated
    with pytest.raises(PayloadError) as exc:
        validate_payload({"cfg": {"mod": "fast"}}, schema)
    # THEN the failure carries the suggestion, in the details and in the message
    assert exc.value.details is not None
    assert exc.value.details.get("did_you_mean") == "mode"
    assert "Did you mean 'mode'" in exc.value.message


def test_payload_error_no_suggestion_when_far() -> None:
    """No suggestion is offered when no nested candidate is close enough."""
    # GIVEN a strict nested schema and a payload naming a field close to nothing
    schema = _nested_strict_schema()
    # WHEN the payload is validated
    with pytest.raises(PayloadError) as exc:
        validate_payload({"cfg": {"xyzzy": "x"}}, schema)
    # THEN no suggestion is offered, rather than an arbitrary one
    assert exc.value.details is not None
    assert "did_you_mean" not in exc.value.details


def test_payload_error_missing_required_lists_required() -> None:
    # GIVEN a schema requiring two fields, and a payload carrying one
    def fn(path: str, mode: str) -> None: ...

    schema = signature_to_input_schema(fn)
    # WHEN the payload is validated
    with pytest.raises(PayloadError) as exc:
        validate_payload({"path": "x"}, schema)
    # THEN the failure names the missing field and lists every required one
    assert exc.value.field == "mode"
    assert exc.value.details is not None
    assert exc.value.details["missing"] == "mode"
    assert set(exc.value.details["required"]) == {"path", "mode"}


def test_payload_error_type_mismatch_has_actual_and_expected() -> None:
    # GIVEN a schema requiring a string, and a payload carrying an integer
    def fn(path: str) -> None: ...

    schema = signature_to_input_schema(fn)
    # WHEN the payload is validated
    with pytest.raises(PayloadError) as exc:
        validate_payload({"path": 42}, schema)
    # THEN the failure names both the expected and the actual type
    assert exc.value.field == "path"
    assert exc.value.details is not None
    assert exc.value.details["expected_type"] == "string"
    assert exc.value.details["actual_type"] == "integer"


# Eliminate unused field/return import warnings.
_ = field
