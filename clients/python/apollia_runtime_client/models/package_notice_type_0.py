from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.package_notice_type_0_code import PackageNoticeType0Code

T = TypeVar("T", bound="PackageNoticeType0")


@_attrs_define
class PackageNoticeType0:
    """The package is not signed: nothing says who made it or that it was
    not altered.

        Attributes:
            code (PackageNoticeType0Code):
    """

    code: PackageNoticeType0Code
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        code = self.code.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "code": code,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        code = PackageNoticeType0Code(d.pop("code"))

        package_notice_type_0 = cls(
            code=code,
        )

        package_notice_type_0.additional_properties = d
        return package_notice_type_0

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
