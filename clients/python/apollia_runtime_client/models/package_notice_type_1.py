from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.package_notice_type_1_code import PackageNoticeType1Code

T = TypeVar("T", bound="PackageNoticeType1")


@_attrs_define
class PackageNoticeType1:
    """The signing key is not in the keyring: the publisher name is declared,
    not verified.

        Attributes:
            code (PackageNoticeType1Code):
            fingerprint (str): Fingerprint of the signing key.
    """

    code: PackageNoticeType1Code
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
        code = PackageNoticeType1Code(d.pop("code"))

        fingerprint = d.pop("fingerprint")

        package_notice_type_1 = cls(
            code=code,
            fingerprint=fingerprint,
        )

        package_notice_type_1.additional_properties = d
        return package_notice_type_1

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
