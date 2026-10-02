from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="ProfileEntryView")


@_attrs_define
class ProfileEntryView:
    """One profile entry, as the profile page draws it.

    Attributes:
        created_at (str): ISO 8601 creation timestamp.
        in_schema (bool): `true` when the key is a field of the canonical profile schema.
        key (str): Flat storage key.
        updated_at (str): ISO 8601 last-update timestamp.
        value (str): Plain-text value.
        written_by (str): Provenance tag (`onboarding`, `user`, `agent:<name>`).
    """

    created_at: str
    in_schema: bool
    key: str
    updated_at: str
    value: str
    written_by: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        created_at = self.created_at

        in_schema = self.in_schema

        key = self.key

        updated_at = self.updated_at

        value = self.value

        written_by = self.written_by

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "created_at": created_at,
                "in_schema": in_schema,
                "key": key,
                "updated_at": updated_at,
                "value": value,
                "written_by": written_by,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        created_at = d.pop("created_at")

        in_schema = d.pop("in_schema")

        key = d.pop("key")

        updated_at = d.pop("updated_at")

        value = d.pop("value")

        written_by = d.pop("written_by")

        profile_entry_view = cls(
            created_at=created_at,
            in_schema=in_schema,
            key=key,
            updated_at=updated_at,
            value=value,
            written_by=written_by,
        )

        profile_entry_view.additional_properties = d
        return profile_entry_view

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
