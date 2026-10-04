from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.config_diagnostic_body import ConfigDiagnosticBody


T = TypeVar("T", bound="ConfigErrorBody")


@_attrs_define
class ConfigErrorBody:
    """The error of a configuration route.

    Attributes:
        code (str): `modified`, `invalid`, `locked`, `unavailable` or `internal`.
        diagnostics (list[ConfigDiagnosticBody]): The faults of the text, for `invalid` and `locked`.
        error (str): What went wrong, for the user; the same sentence the command line
            prints for the same refusal.
    """

    code: str
    diagnostics: list[ConfigDiagnosticBody]
    error: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        code = self.code

        diagnostics = []
        for diagnostics_item_data in self.diagnostics:
            diagnostics_item = diagnostics_item_data.to_dict()
            diagnostics.append(diagnostics_item)

        error = self.error

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "code": code,
                "diagnostics": diagnostics,
                "error": error,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.config_diagnostic_body import ConfigDiagnosticBody

        d = dict(src_dict)
        code = d.pop("code")

        diagnostics = []
        _diagnostics = d.pop("diagnostics")
        for diagnostics_item_data in _diagnostics:
            diagnostics_item = ConfigDiagnosticBody.from_dict(diagnostics_item_data)

            diagnostics.append(diagnostics_item)

        error = d.pop("error")

        config_error_body = cls(
            code=code,
            diagnostics=diagnostics,
            error=error,
        )

        config_error_body.additional_properties = d
        return config_error_body

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
