from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="OfferedAccess")


@_attrs_define
class OfferedAccess:
    """One access that exists on the machine.

    Attributes:
        by_default (bool): Whether an agent created without code is granted it by default: a
            tool of a family the default grants, or one the default names.
        description (str): What it does, in one line.
        family (str): Its action family, as the journal names it.
        id (str): The identifier an agent is granted it by.
        sensitive (bool): Whether its family is anything but plain reading.
    """

    by_default: bool
    description: str
    family: str
    id: str
    sensitive: bool
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        by_default = self.by_default

        description = self.description

        family = self.family

        id = self.id

        sensitive = self.sensitive

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "by_default": by_default,
                "description": description,
                "family": family,
                "id": id,
                "sensitive": sensitive,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        by_default = d.pop("by_default")

        description = d.pop("description")

        family = d.pop("family")

        id = d.pop("id")

        sensitive = d.pop("sensitive")

        offered_access = cls(
            by_default=by_default,
            description=description,
            family=family,
            id=id,
            sensitive=sensitive,
        )

        offered_access.additional_properties = d
        return offered_access

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
