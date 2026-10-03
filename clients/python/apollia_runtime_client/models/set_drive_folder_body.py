from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="SetDriveFolderBody")


@_attrs_define
class SetDriveFolderBody:
    """Request body for `PUT /api/v1/connectors/drive-folders/{account_id}`.

    Attributes:
        folder_path (str): Folder path under My Drive, `Documents/Apollia` for instance. Empty
            means the root of My Drive.
    """

    folder_path: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        folder_path = self.folder_path

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "folder_path": folder_path,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        folder_path = d.pop("folder_path")

        set_drive_folder_body = cls(
            folder_path=folder_path,
        )

        set_drive_folder_body.additional_properties = d
        return set_drive_folder_body

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
