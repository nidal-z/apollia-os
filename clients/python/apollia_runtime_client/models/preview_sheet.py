from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="PreviewSheet")


@_attrs_define
class PreviewSheet:
    """One sheet of the file.

    Attributes:
        name (str): Name of the sheet.
        non_empty_rows (int): How many of its rows hold at least one non-blank cell.
    """

    name: str
    non_empty_rows: int
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        name = self.name

        non_empty_rows = self.non_empty_rows

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "name": name,
                "nonEmptyRows": non_empty_rows,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        name = d.pop("name")

        non_empty_rows = d.pop("nonEmptyRows")

        preview_sheet = cls(
            name=name,
            non_empty_rows=non_empty_rows,
        )

        preview_sheet.additional_properties = d
        return preview_sheet

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
