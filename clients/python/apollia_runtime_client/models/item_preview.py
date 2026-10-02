from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="ItemPreview")


@_attrs_define
class ItemPreview:
    """One Python tool of the package.

    Attributes:
        description (str): What it does.
        family (str): Action family: `read`, `read_personal`, `write_or_delete`,
            `send_external` or `execute_code`.
        name (str): Identifier, unique in the package.
        sensitive (bool): Its family is sensitive.
    """

    description: str
    family: str
    name: str
    sensitive: bool
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        description = self.description

        family = self.family

        name = self.name

        sensitive = self.sensitive

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "description": description,
                "family": family,
                "name": name,
                "sensitive": sensitive,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        description = d.pop("description")

        family = d.pop("family")

        name = d.pop("name")

        sensitive = d.pop("sensitive")

        item_preview = cls(
            description=description,
            family=family,
            name=name,
            sensitive=sensitive,
        )

        item_preview.additional_properties = d
        return item_preview

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
