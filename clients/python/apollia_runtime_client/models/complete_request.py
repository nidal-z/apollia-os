from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.complete_request_schema_type_0 import CompleteRequestSchemaType0
    from ..models.message_dto import MessageDto


T = TypeVar("T", bound="CompleteRequest")


@_attrs_define
class CompleteRequest:
    """Request body for `POST /api/v1/llm/complete`.

    Attributes:
        messages (list[MessageDto]): Ordered list of messages forming the conversation history.
        backend (None | str | Unset): Backend to use; falls back to the router default if omitted.
        grammar (None | str | Unset): Optional GBNF grammar constraining the decoding (local backends).
        schema (CompleteRequestSchemaType0 | None | Unset): JSON Schema the answer must satisfy, or `null` for free-form
            generation.

            A `grammar` already on the request wins over it, being the more specific
            constraint of the two; the answer is validated against the schema either
            way and comes back in `value`.
        seed (int | None | Unset): Sampling seed, for a reproducible answer on a backend that carries one.
        temperature (float | None | Unset): Sampling temperature; the backend default when omitted.
    """

    messages: list[MessageDto]
    backend: None | str | Unset = UNSET
    grammar: None | str | Unset = UNSET
    schema: CompleteRequestSchemaType0 | None | Unset = UNSET
    seed: int | None | Unset = UNSET
    temperature: float | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.complete_request_schema_type_0 import CompleteRequestSchemaType0

        messages = []
        for messages_item_data in self.messages:
            messages_item = messages_item_data.to_dict()
            messages.append(messages_item)

        backend: None | str | Unset
        if isinstance(self.backend, Unset):
            backend = UNSET
        else:
            backend = self.backend

        grammar: None | str | Unset
        if isinstance(self.grammar, Unset):
            grammar = UNSET
        else:
            grammar = self.grammar

        schema: dict[str, Any] | None | Unset
        if isinstance(self.schema, Unset):
            schema = UNSET
        elif isinstance(self.schema, CompleteRequestSchemaType0):
            schema = self.schema.to_dict()
        else:
            schema = self.schema

        seed: int | None | Unset
        if isinstance(self.seed, Unset):
            seed = UNSET
        else:
            seed = self.seed

        temperature: float | None | Unset
        if isinstance(self.temperature, Unset):
            temperature = UNSET
        else:
            temperature = self.temperature

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "messages": messages,
            }
        )
        if backend is not UNSET:
            field_dict["backend"] = backend
        if grammar is not UNSET:
            field_dict["grammar"] = grammar
        if schema is not UNSET:
            field_dict["schema"] = schema
        if seed is not UNSET:
            field_dict["seed"] = seed
        if temperature is not UNSET:
            field_dict["temperature"] = temperature

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.complete_request_schema_type_0 import CompleteRequestSchemaType0
        from ..models.message_dto import MessageDto

        d = dict(src_dict)
        messages = []
        _messages = d.pop("messages")
        for messages_item_data in _messages:
            messages_item = MessageDto.from_dict(messages_item_data)

            messages.append(messages_item)

        def _parse_backend(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        backend = _parse_backend(d.pop("backend", UNSET))

        def _parse_grammar(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        grammar = _parse_grammar(d.pop("grammar", UNSET))

        def _parse_schema(data: object) -> CompleteRequestSchemaType0 | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                schema_type_0 = CompleteRequestSchemaType0.from_dict(data)

                return schema_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(CompleteRequestSchemaType0 | None | Unset, data)

        schema = _parse_schema(d.pop("schema", UNSET))

        def _parse_seed(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        seed = _parse_seed(d.pop("seed", UNSET))

        def _parse_temperature(data: object) -> float | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(float | None | Unset, data)

        temperature = _parse_temperature(d.pop("temperature", UNSET))

        complete_request = cls(
            messages=messages,
            backend=backend,
            grammar=grammar,
            schema=schema,
            seed=seed,
            temperature=temperature,
        )

        complete_request.additional_properties = d
        return complete_request

    @property
    def additional_keys(self) -> list[str]:
        return list(self.additional_properties.keys())

    def __getitem__(self, key: str) -> Any:
        return self.additional_properties[key]

    def __setitem__(self, key: str, value: Any) -> None:
        self.additional_properties[key] = value

    def __delitem__(self, key: str) -> None:
        del self.additional_properties[key]

    def __contains__(self, key: str) -> bool:
        return key in self.additional_properties
