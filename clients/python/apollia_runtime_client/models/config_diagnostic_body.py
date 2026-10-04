from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="ConfigDiagnosticBody")


@_attrs_define
class ConfigDiagnosticBody:
    """One fault of a configuration text, at its line.

    Attributes:
        column (int): 1-based column, in characters.
        line (int): 1-based line.
        message (str): What is wrong, in English.
        path (str): Dotted path of the key at fault; empty at the root.
    """

    column: int
    line: int
    message: str
    path: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        column = self.column

        line = self.line

        message = self.message

        path = self.path

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "column": column,
                "line": line,
                "message": message,
                "path": path,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        column = d.pop("column")

        line = d.pop("line")

        message = d.pop("message")

        path = d.pop("path")

        config_diagnostic_body = cls(
            column=column,
            line=line,
            message=message,
            path=path,
        )

        config_diagnostic_body.additional_properties = d
        return config_diagnostic_body

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
