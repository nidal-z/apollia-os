from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="DriveFolderStatus")


@_attrs_define
class DriveFolderStatus:
    """The Drive folder of one connected Google account.

    Attributes:
        account_id (str): Account identifier, the Google email.
        effective_folder_path (str): The path the executors walk: the user's choice, or the default.
        folder_path (None | str | Unset): The path the user chose, absent when the default applies. An empty
            path means the root of My Drive.
    """

    account_id: str
    effective_folder_path: str
    folder_path: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        account_id = self.account_id

        effective_folder_path = self.effective_folder_path

        folder_path: None | str | Unset
        if isinstance(self.folder_path, Unset):
            folder_path = UNSET
        else:
            folder_path = self.folder_path

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "account_id": account_id,
                "effective_folder_path": effective_folder_path,
            }
        )
        if folder_path is not UNSET:
            field_dict["folder_path"] = folder_path

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        account_id = d.pop("account_id")

        effective_folder_path = d.pop("effective_folder_path")

        def _parse_folder_path(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        folder_path = _parse_folder_path(d.pop("folder_path", UNSET))

        drive_folder_status = cls(
            account_id=account_id,
            effective_folder_path=effective_folder_path,
            folder_path=folder_path,
        )

        drive_folder_status.additional_properties = d
        return drive_folder_status

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
