from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast






T = TypeVar("T", bound="TokenUsageResponse")



@_attrs_define
class TokenUsageResponse:
    """ Token usage included in chat responses.

        Attributes:
            completion_tokens (int): Number of tokens in the completion.
            prompt_tokens (int): Number of tokens in the prompt.
            cost_usd (float | None | Unset): Estimated cost in USD (cloud backends only; `None` for local inference).
     """

    completion_tokens: int
    prompt_tokens: int
    cost_usd: float | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        completion_tokens = self.completion_tokens

        prompt_tokens = self.prompt_tokens

        cost_usd: float | None | Unset
        if isinstance(self.cost_usd, Unset):
            cost_usd = UNSET
        else:
            cost_usd = self.cost_usd


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "completion_tokens": completion_tokens,
            "prompt_tokens": prompt_tokens,
        })
        if cost_usd is not UNSET:
            field_dict["cost_usd"] = cost_usd

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        completion_tokens = d.pop("completion_tokens")

        prompt_tokens = d.pop("prompt_tokens")

        def _parse_cost_usd(data: object) -> float | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(float | None | Unset, data)

        cost_usd = _parse_cost_usd(d.pop("cost_usd", UNSET))


        token_usage_response = cls(
            completion_tokens=completion_tokens,
            prompt_tokens=prompt_tokens,
            cost_usd=cost_usd,
        )


        token_usage_response.additional_properties = d
        return token_usage_response

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
