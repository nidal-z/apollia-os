from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset







T = TypeVar("T", bound="DailyCostEntry")



@_attrs_define
class DailyCostEntry:
    """ A single day+backend cost entry for the daily chart.

        Attributes:
            backend (str): Backend name.
            cost_usd (float): Total estimated cost in USD for this day.
            date (str): Local calendar day of the host, in `YYYY-MM-DD` format.
     """

    backend: str
    cost_usd: float
    date: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        backend = self.backend

        cost_usd = self.cost_usd

        date = self.date


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "backend": backend,
            "cost_usd": cost_usd,
            "date": date,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        backend = d.pop("backend")

        cost_usd = d.pop("cost_usd")

        date = d.pop("date")

        daily_cost_entry = cls(
            backend=backend,
            cost_usd=cost_usd,
            date=date,
        )


        daily_cost_entry.additional_properties = d
        return daily_cost_entry

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
