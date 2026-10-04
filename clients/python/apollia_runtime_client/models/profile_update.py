from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.profile_update_fields import ProfileUpdateFields


T = TypeVar("T", bound="ProfileUpdate")


@_attrs_define
class ProfileUpdate:
    """A change to a profile: a new name, new field values, or both. An empty
    value empties its field.

        Attributes:
            fields (ProfileUpdateFields | Unset): New values, by field key.
            name (None | str | Unset): New display name.
    """

    fields: ProfileUpdateFields | Unset = UNSET
    name: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        fields: dict[str, Any] | Unset = UNSET
        if not isinstance(self.fields, Unset):
            fields = self.fields.to_dict()

        name: None | str | Unset
        if isinstance(self.name, Unset):
            name = UNSET
        else:
            name = self.name

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({})
        if fields is not UNSET:
            field_dict["fields"] = fields
        if name is not UNSET:
            field_dict["name"] = name

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.profile_update_fields import ProfileUpdateFields

        d = dict(src_dict)
        _fields = d.pop("fields", UNSET)
        fields: ProfileUpdateFields | Unset
        if isinstance(_fields, Unset):
            fields = UNSET
        else:
            fields = ProfileUpdateFields.from_dict(_fields)

        def _parse_name(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        name = _parse_name(d.pop("name", UNSET))

        profile_update = cls(
            fields=fields,
            name=name,
        )

        profile_update.additional_properties = d
        return profile_update

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
