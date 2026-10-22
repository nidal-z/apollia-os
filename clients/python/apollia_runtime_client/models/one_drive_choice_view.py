from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="OneDriveChoiceView")


@_attrs_define
class OneDriveChoiceView:
    """One chosen folder or file, as a client reads it.

    Attributes:
        id (str): Graph item id.
        is_folder (bool): `true` for a folder, which admits everything under it.
        name (str): Name at the time of the choice.
        path (str): Path of the item in the drive, `/drive/root:/Documents/Clients`.
    """

    id: str
    is_folder: bool
    name: str
    path: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        id = self.id

        is_folder = self.is_folder

        name = self.name

        path = self.path

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "id": id,
                "is_folder": is_folder,
                "name": name,
                "path": path,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        id = d.pop("id")

        is_folder = d.pop("is_folder")

        name = d.pop("name")

        path = d.pop("path")

        one_drive_choice_view = cls(
            id=id,
            is_folder=is_folder,
            name=name,
            path=path,
        )

        one_drive_choice_view.additional_properties = d
        return one_drive_choice_view

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
