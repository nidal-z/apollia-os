from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.engine_entry import EngineEntry


T = TypeVar("T", bound="EngineInstalledResponse")


@_attrs_define
class EngineInstalledResponse:
    """Engines installed by a request.

    Attributes:
        installed (list[EngineEntry]): What was installed.
    """

    installed: list[EngineEntry]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        installed = []
        for installed_item_data in self.installed:
            installed_item = installed_item_data.to_dict()
            installed.append(installed_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "installed": installed,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.engine_entry import EngineEntry

        d = dict(src_dict)
        installed = []
        _installed = d.pop("installed")
        for installed_item_data in _installed:
            installed_item = EngineEntry.from_dict(installed_item_data)

            installed.append(installed_item)

        engine_installed_response = cls(
            installed=installed,
        )

        engine_installed_response.additional_properties = d
        return engine_installed_response

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
