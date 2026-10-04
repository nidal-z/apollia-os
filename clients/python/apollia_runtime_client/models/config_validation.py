from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.config_diagnostic_body import ConfigDiagnosticBody


T = TypeVar("T", bound="ConfigValidation")


@_attrs_define
class ConfigValidation:
    """The verdict on a text.

    Attributes:
        diagnostics (list[ConfigDiagnosticBody]): Every fault, each at its line.
        valid (bool): Whether the writer would accept the text.
    """

    diagnostics: list[ConfigDiagnosticBody]
    valid: bool
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        diagnostics = []
        for diagnostics_item_data in self.diagnostics:
            diagnostics_item = diagnostics_item_data.to_dict()
            diagnostics.append(diagnostics_item)

        valid = self.valid

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "diagnostics": diagnostics,
                "valid": valid,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.config_diagnostic_body import ConfigDiagnosticBody

        d = dict(src_dict)
        diagnostics = []
        _diagnostics = d.pop("diagnostics")
        for diagnostics_item_data in _diagnostics:
            diagnostics_item = ConfigDiagnosticBody.from_dict(diagnostics_item_data)

            diagnostics.append(diagnostics_item)

        valid = d.pop("valid")

        config_validation = cls(
            diagnostics=diagnostics,
            valid=valid,
        )

        config_validation.additional_properties = d
        return config_validation

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
