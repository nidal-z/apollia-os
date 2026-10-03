from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.install_refusal_type_2_code import InstallRefusalType2Code

T = TypeVar("T", bound="InstallRefusalType2")


@_attrs_define
class InstallRefusalType2:
    """Governance requires a signed package, the organisation policy names the
    publishers it trusts, and the key that signed this one is not among
    them.

        Attributes:
            code (InstallRefusalType2Code):
            fingerprint (str): Fingerprint of the signing key.
    """

    code: InstallRefusalType2Code
    fingerprint: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        code = self.code.value

        fingerprint = self.fingerprint

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "code": code,
                "fingerprint": fingerprint,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        code = InstallRefusalType2Code(d.pop("code"))

        fingerprint = d.pop("fingerprint")

        install_refusal_type_2 = cls(
            code=code,
            fingerprint=fingerprint,
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
