from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..models.timeline_event_type_3_type import TimelineEventType3Type
from ..types import UNSET, Unset
from typing import cast






T = TypeVar("T", bound="TimelineEventType3")



@_attrs_define
class TimelineEventType3:
    """ Recorded LLM call.

        Attributes:
            backend (str): Backend name (e.g. `"anthropic"`, `"local"`).
            model (str): Model identifier.
            timestamp (str): ISO 8601 timestamp.
            type_ (TimelineEventType3Type):
            completion_tokens (int | None | Unset): Completion tokens.
            cost_usd (float | None | Unset): Estimated cost in USD.
            latency_ms (int | None | Unset): Latency in milliseconds.
            prompt_tokens (int | None | Unset): Prompt tokens.
     """

    backend: str
    model: str
    timestamp: str
    type_: TimelineEventType3Type
    completion_tokens: int | None | Unset = UNSET
    cost_usd: float | None | Unset = UNSET
    latency_ms: int | None | Unset = UNSET
    prompt_tokens: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        backend = self.backend

        model = self.model

        timestamp = self.timestamp

        type_ = self.type_.value

        completion_tokens: int | None | Unset
        if isinstance(self.completion_tokens, Unset):
            completion_tokens = UNSET
        else:
            completion_tokens = self.completion_tokens

        cost_usd: float | None | Unset
        if isinstance(self.cost_usd, Unset):
            cost_usd = UNSET
        else:
            cost_usd = self.cost_usd

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


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "backend": backend,
            "model": model,
            "timestamp": timestamp,
            "type": type_,
        })
        if completion_tokens is not UNSET:
            field_dict["completion_tokens"] = completion_tokens
        if cost_usd is not UNSET:
            field_dict["cost_usd"] = cost_usd
        if latency_ms is not UNSET:
            field_dict["latency_ms"] = latency_ms
        if prompt_tokens is not UNSET:
            field_dict["prompt_tokens"] = prompt_tokens

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        backend = d.pop("backend")

        model = d.pop("model")

        timestamp = d.pop("timestamp")

        type_ = TimelineEventType3Type(d.pop("type"))




        def _parse_completion_tokens(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        completion_tokens = _parse_completion_tokens(d.pop("completion_tokens", UNSET))


        def _parse_cost_usd(data: object) -> float | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(float | None | Unset, data)

        cost_usd = _parse_cost_usd(d.pop("cost_usd", UNSET))


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


        timeline_event_type_3 = cls(
            backend=backend,
            model=model,
            timestamp=timestamp,
            type_=type_,
            completion_tokens=completion_tokens,
            cost_usd=cost_usd,
            latency_ms=latency_ms,
            prompt_tokens=prompt_tokens,
        )


        timeline_event_type_3.additional_properties = d
        return timeline_event_type_3

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
