from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="PreviewSourceBody")


@_attrs_define
class PreviewSourceBody:
    """Body of `POST /api/v1/sources/preview`.

    Attributes:
        path (str): Absolute path of a `.xlsx`, `.xlsm`, `.xlsb`, `.xls`, `.ods` or `.csv` file.
        header_row (int | None | Unset): The header row, from 1, on the shown sheet; the proposed row when absent.
        sheet (None | str | Unset): The sheet to show; the proposed sheet when absent.
    """

    path: str
    header_row: int | None | Unset = UNSET
    sheet: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        path = self.path

        header_row: int | None | Unset
        if isinstance(self.header_row, Unset):
            header_row = UNSET
        else:
            header_row = self.header_row

        sheet: None | str | Unset
        if isinstance(self.sheet, Unset):
            sheet = UNSET
        else:
            sheet = self.sheet

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "path": path,
            }
        )
        if header_row is not UNSET:
            field_dict["headerRow"] = header_row
        if sheet is not UNSET:
            field_dict["sheet"] = sheet

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        path = d.pop("path")

        def _parse_header_row(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        header_row = _parse_header_row(d.pop("headerRow", UNSET))

        def _parse_sheet(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        sheet = _parse_sheet(d.pop("sheet", UNSET))

        preview_source_body = cls(
            path=path,
            header_row=header_row,
            sheet=sheet,
        )

        preview_source_body.additional_properties = d
        return preview_source_body

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
