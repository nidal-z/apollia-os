from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.install_refusal_type_2_code import InstallRefusalType2Code

T = TypeVar("T", bound="InstallRefusalType2")


@_attrs_define
class InstallRefusalType2:
    """The incoming version is older than the installed one.

    Attributes:
        code (InstallRefusalType2Code):
        incoming (str): Incoming version.
        installed (str): Installed version.
    """

    code: InstallRefusalType2Code
    incoming: str
    installed: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        code = self.code.value

        incoming = self.incoming

        installed = self.installed

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "code": code,
                "incoming": incoming,
                "installed": installed,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        code = InstallRefusalType2Code(d.pop("code"))

        incoming = d.pop("incoming")

        installed = d.pop("installed")

        install_refusal_type_2 = cls(
            code=code,
            incoming=incoming,
            installed=installed,
        )

        install_refusal_type_2.additional_properties = d
        return install_refusal_type_2

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
