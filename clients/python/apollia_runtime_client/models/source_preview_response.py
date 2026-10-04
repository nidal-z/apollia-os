from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.preview_column import PreviewColumn
    from ..models.preview_row_view import PreviewRowView
    from ..models.preview_sheet import PreviewSheet


T = TypeVar("T", bound="SourcePreviewResponse")


@_attrs_define
class SourcePreviewResponse:
    """What a batch on the file would read.

    Attributes:
        can_modify_original (bool): True for `.xlsx` and `.xlsm`: results may be written into the original.
        columns (list[PreviewColumn]): The columns under the header row.
        estimated_items (int): How many items a batch would read: the non-empty rows below the header.
        format_ (str): `xlsx`, `xlsm`, `xlsb`, `xls`, `ods` or `csv`.
        formatting_not_kept (bool): True for `.xls`, `.xlsb` and `.ods`: the copy of results is a new
            workbook without the original formatting.
        rows (list[PreviewRowView]): The first twenty rows of the shown sheet, from its top.
        sheets (list[PreviewSheet]): Every sheet, in the file's order; a CSV file has one, named after it.
        source_type (str): The batch source type that reads the file: `spreadsheet` or `csv`.
        header_row (int | None | Unset): The header row used: the one asked for, or the proposed one.
        proposed_header_row (int | None | Unset): The header row proposed on the shown sheet.
        proposed_sheet (None | str | Unset): The sheet proposed: the only one, or the one with the most non-empty rows.
        sheet (None | str | Unset): The sheet shown: the one asked for, or the proposed one.
    """

    can_modify_original: bool
    columns: list[PreviewColumn]
    estimated_items: int
    format_: str
    formatting_not_kept: bool
    rows: list[PreviewRowView]
    sheets: list[PreviewSheet]
    source_type: str
    header_row: int | None | Unset = UNSET
    proposed_header_row: int | None | Unset = UNSET
    proposed_sheet: None | str | Unset = UNSET
    sheet: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        can_modify_original = self.can_modify_original

        columns = []
        for columns_item_data in self.columns:
            columns_item = columns_item_data.to_dict()
            columns.append(columns_item)

        estimated_items = self.estimated_items

        format_ = self.format_

        formatting_not_kept = self.formatting_not_kept

        rows = []
        for rows_item_data in self.rows:
            rows_item = rows_item_data.to_dict()
            rows.append(rows_item)

        sheets = []
        for sheets_item_data in self.sheets:
            sheets_item = sheets_item_data.to_dict()
            sheets.append(sheets_item)

        source_type = self.source_type

        header_row: int | None | Unset
        if isinstance(self.header_row, Unset):
            header_row = UNSET
        else:
            header_row = self.header_row

        proposed_header_row: int | None | Unset
        if isinstance(self.proposed_header_row, Unset):
            proposed_header_row = UNSET
        else:
            proposed_header_row = self.proposed_header_row

        proposed_sheet: None | str | Unset
        if isinstance(self.proposed_sheet, Unset):
            proposed_sheet = UNSET
        else:
            proposed_sheet = self.proposed_sheet

        sheet: None | str | Unset
        if isinstance(self.sheet, Unset):
            sheet = UNSET
        else:
            sheet = self.sheet

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "canModifyOriginal": can_modify_original,
                "columns": columns,
                "estimatedItems": estimated_items,
                "format": format_,
                "formattingNotKept": formatting_not_kept,
                "rows": rows,
                "sheets": sheets,
                "sourceType": source_type,
            }
        )
        if header_row is not UNSET:
            field_dict["headerRow"] = header_row
        if proposed_header_row is not UNSET:
            field_dict["proposedHeaderRow"] = proposed_header_row
        if proposed_sheet is not UNSET:
            field_dict["proposedSheet"] = proposed_sheet
        if sheet is not UNSET:
            field_dict["sheet"] = sheet

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.preview_column import PreviewColumn
        from ..models.preview_row_view import PreviewRowView
        from ..models.preview_sheet import PreviewSheet

        d = dict(src_dict)
        can_modify_original = d.pop("canModifyOriginal")

        columns = []
        _columns = d.pop("columns")
        for columns_item_data in _columns:
            columns_item = PreviewColumn.from_dict(columns_item_data)

            columns.append(columns_item)

        estimated_items = d.pop("estimatedItems")

        format_ = d.pop("format")

        formatting_not_kept = d.pop("formattingNotKept")

        rows = []
        _rows = d.pop("rows")
        for rows_item_data in _rows:
            rows_item = PreviewRowView.from_dict(rows_item_data)

            rows.append(rows_item)

        sheets = []
        _sheets = d.pop("sheets")
        for sheets_item_data in _sheets:
            sheets_item = PreviewSheet.from_dict(sheets_item_data)

            sheets.append(sheets_item)

        source_type = d.pop("sourceType")

        def _parse_header_row(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        header_row = _parse_header_row(d.pop("headerRow", UNSET))

        def _parse_proposed_header_row(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        proposed_header_row = _parse_proposed_header_row(d.pop("proposedHeaderRow", UNSET))

        def _parse_proposed_sheet(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        proposed_sheet = _parse_proposed_sheet(d.pop("proposedSheet", UNSET))

        def _parse_sheet(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        sheet = _parse_sheet(d.pop("sheet", UNSET))

        source_preview_response = cls(
            can_modify_original=can_modify_original,
            columns=columns,
            estimated_items=estimated_items,
            format_=format_,
            formatting_not_kept=formatting_not_kept,
            rows=rows,
            sheets=sheets,
            source_type=source_type,
            header_row=header_row,
            proposed_header_row=proposed_header_row,
            proposed_sheet=proposed_sheet,
            sheet=sheet,
        )

        source_preview_response.additional_properties = d
        return source_preview_response

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
