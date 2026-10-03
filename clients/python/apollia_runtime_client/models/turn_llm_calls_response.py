from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.turn_llm_call import TurnLlmCall


T = TypeVar("T", bound="TurnLlmCallsResponse")


@_attrs_define
class TurnLlmCallsResponse:
    """Response body for `GET /api/v1/sessions/{id}/turns/{turn_id}/llm-calls`.

    Attributes:
        calls (list[TurnLlmCall]): The calls, in the order they were made. Empty for a turn that made
            none or that is unknown.
    """

    calls: list[TurnLlmCall]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        calls = []
        for calls_item_data in self.calls:
            calls_item = calls_item_data.to_dict()
            calls.append(calls_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "calls": calls,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.turn_llm_call import TurnLlmCall

        d = dict(src_dict)
        calls = []
        _calls = d.pop("calls")
        for calls_item_data in _calls:
            calls_item = TurnLlmCall.from_dict(calls_item_data)

            calls.append(calls_item)

        turn_llm_calls_response = cls(
            calls=calls,
        )

        turn_llm_calls_response.additional_properties = d
        return turn_llm_calls_response

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
