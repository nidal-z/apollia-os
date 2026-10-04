from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="StoredCredential")


@_attrs_define
class StoredCredential:
    """Answer of `PUT /api/v1/tool-credentials/{tool_name}/{key_name}`.

    Attributes:
        key_name (str): Key name stored.
        tool_name (str): Owning tool.
    """

    key_name: str
    tool_name: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        key_name = self.key_name

        tool_name = self.tool_name

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "key_name": key_name,
                "tool_name": tool_name,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        key_name = d.pop("key_name")

        tool_name = d.pop("tool_name")

        stored_credential = cls(
            key_name=key_name,
            tool_name=tool_name,
        )

        stored_credential.additional_properties = d
        return stored_credential

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
