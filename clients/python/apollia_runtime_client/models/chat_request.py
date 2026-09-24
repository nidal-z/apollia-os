from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.chat_request_schema_type_0 import ChatRequestSchemaType0


T = TypeVar("T", bound="ChatRequest")


@_attrs_define
class ChatRequest:
    """Request body for `POST /api/v1/llm/chat`.

    Attributes:
        prompt (str): User prompt text to send to the LLM.
        backend (None | str | Unset): Backend to use; falls back to the router default if omitted.
        schema (ChatRequestSchemaType0 | None | Unset): JSON Schema the answer must satisfy, or `null` for free-form
            generation.

            Same contract as `ctx.llm(schema=...)`: the backend turns it into
            whatever decoding constraint it supports, the answer is validated
            against it, and the validated value comes back in `value`.
        seed (int | None | Unset): Sampling seed, for a reproducible answer on a backend that carries one.
        temperature (float | None | Unset): Sampling temperature; the backend default when omitted.
    """

    prompt: str
    backend: None | str | Unset = UNSET
    schema: ChatRequestSchemaType0 | None | Unset = UNSET
    seed: int | None | Unset = UNSET
    temperature: float | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.chat_request_schema_type_0 import ChatRequestSchemaType0

        prompt = self.prompt

        backend: None | str | Unset
        if isinstance(self.backend, Unset):
            backend = UNSET
        else:
            backend = self.backend

        schema: dict[str, Any] | None | Unset
        if isinstance(self.schema, Unset):
            schema = UNSET
        elif isinstance(self.schema, ChatRequestSchemaType0):
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
                "prompt": prompt,
            }
        )
        if backend is not UNSET:
            field_dict["backend"] = backend
        if schema is not UNSET:
            field_dict["schema"] = schema
        if seed is not UNSET:
            field_dict["seed"] = seed
        if temperature is not UNSET:
            field_dict["temperature"] = temperature

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.chat_request_schema_type_0 import ChatRequestSchemaType0

        d = dict(src_dict)
        prompt = d.pop("prompt")

        def _parse_backend(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        backend = _parse_backend(d.pop("backend", UNSET))

        def _parse_schema(data: object) -> ChatRequestSchemaType0 | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                schema_type_0 = ChatRequestSchemaType0.from_dict(data)

                return schema_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(ChatRequestSchemaType0 | None | Unset, data)

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

        chat_request = cls(
            prompt=prompt,
            backend=backend,
            schema=schema,
            seed=seed,
            temperature=temperature,
        )

        chat_request.additional_properties = d
        return chat_request

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
