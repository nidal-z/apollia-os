from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="CredentialEntryView")


@_attrs_define
class CredentialEntryView:
    """A credential as listed: its owner, its key name and its dates, never its
    value.

        Attributes:
            created_at (str): Creation date, RFC 3339.
            key_name (str): Logical key name (e.g. `brave.api_key`).
            tool_name (str): Name of the owning tool.
            last_used_at (None | str | Unset): Date of the last actual use, if any.
    """

    created_at: str
    key_name: str
    tool_name: str
    last_used_at: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        created_at = self.created_at

        key_name = self.key_name

        tool_name = self.tool_name

        last_used_at: None | str | Unset
        if isinstance(self.last_used_at, Unset):
            last_used_at = UNSET
        else:
            last_used_at = self.last_used_at

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "created_at": created_at,
                "key_name": key_name,
                "tool_name": tool_name,
            }
        )
        if last_used_at is not UNSET:
            field_dict["last_used_at"] = last_used_at

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        created_at = d.pop("created_at")

        key_name = d.pop("key_name")

        tool_name = d.pop("tool_name")

        def _parse_last_used_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        last_used_at = _parse_last_used_at(d.pop("last_used_at", UNSET))

        credential_entry_view = cls(
            created_at=created_at,
            key_name=key_name,
            tool_name=tool_name,
            last_used_at=last_used_at,
        )

        credential_entry_view.additional_properties = d
        return credential_entry_view

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
