from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="RequiredConnectionPreview")


@_attrs_define
class RequiredConnectionPreview:
    """A connection the package requires.

    Attributes:
        brought_by_package (bool): The package itself brings an MCP server of that name.
        present (bool): A connection of that type exists on this machine.
        reason (str): Why the package needs it.
        type_ (str): Connection type, as the package names it.
    """

    brought_by_package: bool
    present: bool
    reason: str
    type_: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        brought_by_package = self.brought_by_package

        present = self.present

        reason = self.reason

        type_ = self.type_

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "broughtByPackage": brought_by_package,
                "present": present,
                "reason": reason,
                "type": type_,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        brought_by_package = d.pop("broughtByPackage")

        present = d.pop("present")

        reason = d.pop("reason")

        type_ = d.pop("type")

        required_connection_preview = cls(
            brought_by_package=brought_by_package,
            present=present,
            reason=reason,
            type_=type_,
        )

        required_connection_preview.additional_properties = d
        return required_connection_preview

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
