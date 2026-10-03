from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="TurnLlmCall")


@_attrs_define
class TurnLlmCall:
    """One model call of a turn.

    Attributes:
        backend (str): Logical backend name.
        model (str): Model identifier.
        completion_tokens (int | None | Unset): Tokens generated.
        finish_reason (None | str | Unset): Why the call ended: `stop`, `tool_calls`, `length`, `error` or
            `cancelled`.
        generation_ms (int | None | Unset): Time spent generating in milliseconds; `null` when not measured.
        latency_ms (int | None | Unset): Duration of the call in milliseconds, from dispatch to its last chunk.
        prompt_tokens (int | None | Unset): Tokens read.
        ttft_ms (int | None | Unset): Time to first token in milliseconds; `null` when not measured.
    """

    backend: str
    model: str
    completion_tokens: int | None | Unset = UNSET
    finish_reason: None | str | Unset = UNSET
    generation_ms: int | None | Unset = UNSET
    latency_ms: int | None | Unset = UNSET
    prompt_tokens: int | None | Unset = UNSET
    ttft_ms: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        backend = self.backend

        model = self.model

        completion_tokens: int | None | Unset
        if isinstance(self.completion_tokens, Unset):
            completion_tokens = UNSET
        else:
            completion_tokens = self.completion_tokens

        finish_reason: None | str | Unset
        if isinstance(self.finish_reason, Unset):
            finish_reason = UNSET
        else:
            finish_reason = self.finish_reason

        generation_ms: int | None | Unset
        if isinstance(self.generation_ms, Unset):
            generation_ms = UNSET
        else:
            generation_ms = self.generation_ms

        latency_ms: int | None | Unset
        if isinstance(self.latency_ms, Unset):
            latency_ms = UNSET
        else:
            latency_ms = self.latency_ms

        prompt_tokens: int | None | Unset
        if isinstance(self.prompt_tokens, Unset):
            prompt_tokens = UNSET
        else:
            prompt_tokens = self.prompt_tokens

        ttft_ms: int | None | Unset
        if isinstance(self.ttft_ms, Unset):
            ttft_ms = UNSET
        else:
            ttft_ms = self.ttft_ms

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "backend": backend,
                "model": model,
            }
        )
        if completion_tokens is not UNSET:
            field_dict["completion_tokens"] = completion_tokens
        if finish_reason is not UNSET:
            field_dict["finish_reason"] = finish_reason
        if generation_ms is not UNSET:
            field_dict["generation_ms"] = generation_ms
        if latency_ms is not UNSET:
            field_dict["latency_ms"] = latency_ms
        if prompt_tokens is not UNSET:
            field_dict["prompt_tokens"] = prompt_tokens
        if ttft_ms is not UNSET:
            field_dict["ttft_ms"] = ttft_ms

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        backend = d.pop("backend")

        model = d.pop("model")

        def _parse_completion_tokens(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        completion_tokens = _parse_completion_tokens(d.pop("completion_tokens", UNSET))

        def _parse_finish_reason(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        finish_reason = _parse_finish_reason(d.pop("finish_reason", UNSET))

        def _parse_generation_ms(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        generation_ms = _parse_generation_ms(d.pop("generation_ms", UNSET))

        def _parse_latency_ms(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        latency_ms = _parse_latency_ms(d.pop("latency_ms", UNSET))

        def _parse_prompt_tokens(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        prompt_tokens = _parse_prompt_tokens(d.pop("prompt_tokens", UNSET))

        def _parse_ttft_ms(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        ttft_ms = _parse_ttft_ms(d.pop("ttft_ms", UNSET))

        turn_llm_call = cls(
            backend=backend,
            model=model,
            completion_tokens=completion_tokens,
            finish_reason=finish_reason,
            generation_ms=generation_ms,
            latency_ms=latency_ms,
            prompt_tokens=prompt_tokens,
            ttft_ms=ttft_ms,
        )

        turn_llm_call.additional_properties = d
        return turn_llm_call

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
