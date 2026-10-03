from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.package_notice_type_2_code import PackageNoticeType2Code

T = TypeVar("T", bound="PackageNoticeType2")


@_attrs_define
class PackageNoticeType2:
    """A connection the package requires is missing.

    Attributes:
        code (PackageNoticeType2Code):
        type_ (str): Connection type.
    """

    code: PackageNoticeType2Code
    type_: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        code = self.code.value

        type_ = self.type_

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "code": code,
                "type": type_,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        code = PackageNoticeType2Code(d.pop("code"))

        type_ = d.pop("type")

        package_notice_type_2 = cls(
            code=code,
            type_=type_,
        )

        package_notice_type_2.additional_properties = d
        return package_notice_type_2

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
