from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="ExampleInstallRequest")


@_attrs_define
class ExampleInstallRequest:
    """Body of `POST /api/v1/packages/examples/{name}/install`.

    Attributes:
        changes_confirmed (bool | Unset): The user accepted what an update adds.
        deps_confirmed (bool | Unset): The user accepted the Python dependencies the package installs.
    """

    changes_confirmed: bool | Unset = UNSET
    deps_confirmed: bool | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        changes_confirmed = self.changes_confirmed

        deps_confirmed = self.deps_confirmed

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({})
        if changes_confirmed is not UNSET:
            field_dict["changesConfirmed"] = changes_confirmed
        if deps_confirmed is not UNSET:
            field_dict["depsConfirmed"] = deps_confirmed

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        changes_confirmed = d.pop("changesConfirmed", UNSET)

        deps_confirmed = d.pop("depsConfirmed", UNSET)

        example_install_request = cls(
            changes_confirmed=changes_confirmed,
            deps_confirmed=deps_confirmed,
        )

        example_install_request.additional_properties = d
        return example_install_request

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
