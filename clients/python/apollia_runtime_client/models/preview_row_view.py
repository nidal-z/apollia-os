from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="PreviewRowView")


@_attrs_define
class PreviewRowView:
    """One row of the preview.

    Attributes:
        cells (list[str]): The text of each cell, column A first.
        row (int): Row number, from 1.
    """

    cells: list[str]
    row: int
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        cells = self.cells

        row = self.row

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "cells": cells,
                "row": row,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        cells = cast(list[str], d.pop("cells"))

        row = d.pop("row")

        preview_row_view = cls(
            cells=cells,
            row=row,
        )

        preview_row_view.additional_properties = d
        return preview_row_view

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
