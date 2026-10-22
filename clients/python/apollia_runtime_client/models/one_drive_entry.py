from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="OneDriveEntry")


@_attrs_define
class OneDriveEntry:
    """One item of a folder being browsed.

    Attributes:
        chosen (bool): `true` when the item itself is chosen.
        id (str): Graph item id.
        is_folder (bool): `true` for a folder, which can be opened or chosen whole.
        name (str): Display name.
    """

    chosen: bool
    id: str
    is_folder: bool
    name: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        chosen = self.chosen

        id = self.id

        is_folder = self.is_folder

        name = self.name

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "chosen": chosen,
                "id": id,
                "is_folder": is_folder,
                "name": name,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        chosen = d.pop("chosen")

        id = d.pop("id")

        is_folder = d.pop("is_folder")

        name = d.pop("name")

        one_drive_entry = cls(
            chosen=chosen,
            id=id,
            is_folder=is_folder,
            name=name,
        )

        one_drive_entry.additional_properties = d
        return one_drive_entry

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
