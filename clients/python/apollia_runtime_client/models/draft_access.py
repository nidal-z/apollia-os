from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="DraftAccess")


@_attrs_define
class DraftAccess:
    """One access of a draft.

    Attributes:
        by_default (bool): Whether it is there because an agent created without code is granted
            it by default, rather than because the sheet asked for it.
        checked (bool): Whether the form opens with it granted. A sensitive access is only
            when the default grants it to every new agent.
        id (str): The identifier the agent would be granted it by.
        sensitive (bool): Whether its family is anything but plain reading.
    """

    by_default: bool
    checked: bool
    id: str
    sensitive: bool
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        by_default = self.by_default

        checked = self.checked

        id = self.id

        sensitive = self.sensitive

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "by_default": by_default,
                "checked": checked,
                "id": id,
                "sensitive": sensitive,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        by_default = d.pop("by_default")

        checked = d.pop("checked")

        id = d.pop("id")

        sensitive = d.pop("sensitive")

        draft_access = cls(
            by_default=by_default,
            checked=checked,
            id=id,
            sensitive=sensitive,
        )

        draft_access.additional_properties = d
        return draft_access

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
