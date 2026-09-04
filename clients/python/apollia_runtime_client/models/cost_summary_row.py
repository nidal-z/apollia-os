from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset







T = TypeVar("T", bound="CostSummaryRow")



@_attrs_define
class CostSummaryRow:
    """ A single backend/model cost summary row.

        Attributes:
            backend (str): Backend name.
            call_count (int): Number of LLM calls.
            model (str): Model identifier.
            total_cost_usd (float): Estimated total cost in USD.
            total_tokens (int): Total tokens (prompt + completion).
     """

    backend: str
    call_count: int
    model: str
    total_cost_usd: float
    total_tokens: int
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        backend = self.backend

        call_count = self.call_count

        model = self.model

        total_cost_usd = self.total_cost_usd

        total_tokens = self.total_tokens


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "backend": backend,
            "call_count": call_count,
            "model": model,
            "total_cost_usd": total_cost_usd,
            "total_tokens": total_tokens,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        backend = d.pop("backend")

        call_count = d.pop("call_count")

        model = d.pop("model")

        total_cost_usd = d.pop("total_cost_usd")

        total_tokens = d.pop("total_tokens")

        cost_summary_row = cls(
            backend=backend,
            call_count=call_count,
            model=model,
            total_cost_usd=total_cost_usd,
            total_tokens=total_tokens,
        )


        cost_summary_row.additional_properties = d
        return cost_summary_row

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
