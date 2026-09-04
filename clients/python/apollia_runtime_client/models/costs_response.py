from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from typing import cast

if TYPE_CHECKING:
  from ..models.cost_summary_row import CostSummaryRow





T = TypeVar("T", bound="CostsResponse")



@_attrs_define
class CostsResponse:
    """ Response body for `GET /api/v1/llm/costs`.

        Attributes:
            days (int): Number of days aggregated.
            rows (list[CostSummaryRow]): Per-backend/model cost breakdown.
     """

    days: int
    rows: list[CostSummaryRow]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.cost_summary_row import CostSummaryRow
        days = self.days

        rows = []
        for rows_item_data in self.rows:
            rows_item = rows_item_data.to_dict()
            rows.append(rows_item)




        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "days": days,
            "rows": rows,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.cost_summary_row import CostSummaryRow
        d = dict(src_dict)
        days = d.pop("days")

        rows = []
        _rows = d.pop("rows")
        for rows_item_data in (_rows):
            rows_item = CostSummaryRow.from_dict(rows_item_data)



            rows.append(rows_item)


        costs_response = cls(
            days=days,
            rows=rows,
        )


        costs_response.additional_properties = d
        return costs_response

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
