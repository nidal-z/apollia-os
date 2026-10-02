from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="PackageInstallRequest")


@_attrs_define
class PackageInstallRequest:
    """Body of `POST /api/v1/packages/install`.

    Attributes:
        path (str): Path of the `.apo` file on this machine.
        allow_downgrade (bool | Unset): Install a version older than the installed one.
        changes_confirmed (bool | Unset): The user accepted what an update adds.
        deps_confirmed (bool | Unset): The user accepted the Python dependencies the package installs.
    """

    path: str
    allow_downgrade: bool | Unset = UNSET
    changes_confirmed: bool | Unset = UNSET
    deps_confirmed: bool | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        path = self.path

        allow_downgrade = self.allow_downgrade

        changes_confirmed = self.changes_confirmed

        deps_confirmed = self.deps_confirmed

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "path": path,
            }
        )
        if allow_downgrade is not UNSET:
            field_dict["allowDowngrade"] = allow_downgrade
        if changes_confirmed is not UNSET:
            field_dict["changesConfirmed"] = changes_confirmed
        if deps_confirmed is not UNSET:
            field_dict["depsConfirmed"] = deps_confirmed

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        path = d.pop("path")

        allow_downgrade = d.pop("allowDowngrade", UNSET)

        changes_confirmed = d.pop("changesConfirmed", UNSET)

        deps_confirmed = d.pop("depsConfirmed", UNSET)

        package_install_request = cls(
            path=path,
            allow_downgrade=allow_downgrade,
            changes_confirmed=changes_confirmed,
            deps_confirmed=deps_confirmed,
        )

        package_install_request.additional_properties = d
        return package_install_request

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
