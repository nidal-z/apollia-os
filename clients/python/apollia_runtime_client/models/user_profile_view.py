from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.profile_entry_view import ProfileEntryView


T = TypeVar("T", bound="UserProfileView")


@_attrs_define
class UserProfileView:
    """The whole profile: the schema fields, then the free-form entries.

    Attributes:
        entries (list[ProfileEntryView]): Every entry, in the order the repository lists them.
        extras (list[ProfileEntryView]): Entries outside the schema.
        schema_entries (list[ProfileEntryView]): Entries whose key is a field of the schema.
        last_updated_at (None | str | Unset): The most recent `updated_at` across the entries.
    """

    entries: list[ProfileEntryView]
    extras: list[ProfileEntryView]
    schema_entries: list[ProfileEntryView]
    last_updated_at: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        entries = []
        for entries_item_data in self.entries:
            entries_item = entries_item_data.to_dict()
            entries.append(entries_item)

        extras = []
        for extras_item_data in self.extras:
            extras_item = extras_item_data.to_dict()
            extras.append(extras_item)

        schema_entries = []
        for schema_entries_item_data in self.schema_entries:
            schema_entries_item = schema_entries_item_data.to_dict()
            schema_entries.append(schema_entries_item)

        last_updated_at: None | str | Unset
        if isinstance(self.last_updated_at, Unset):
            last_updated_at = UNSET
        else:
            last_updated_at = self.last_updated_at

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "entries": entries,
                "extras": extras,
                "schema_entries": schema_entries,
            }
        )
        if last_updated_at is not UNSET:
            field_dict["last_updated_at"] = last_updated_at

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.profile_entry_view import ProfileEntryView

        d = dict(src_dict)
        entries = []
        _entries = d.pop("entries")
        for entries_item_data in _entries:
            entries_item = ProfileEntryView.from_dict(entries_item_data)

            entries.append(entries_item)

        extras = []
        _extras = d.pop("extras")
        for extras_item_data in _extras:
            extras_item = ProfileEntryView.from_dict(extras_item_data)

            extras.append(extras_item)

        schema_entries = []
        _schema_entries = d.pop("schema_entries")
        for schema_entries_item_data in _schema_entries:
            schema_entries_item = ProfileEntryView.from_dict(schema_entries_item_data)

            schema_entries.append(schema_entries_item)

        def _parse_last_updated_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        last_updated_at = _parse_last_updated_at(d.pop("last_updated_at", UNSET))

        user_profile_view = cls(
            entries=entries,
            extras=extras,
            schema_entries=schema_entries,
            last_updated_at=last_updated_at,
        )

        user_profile_view.additional_properties = d
        return user_profile_view

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
