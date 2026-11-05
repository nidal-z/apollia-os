from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.engine_rating_reason_type_8_code import EngineRatingReasonType8Code

T = TypeVar("T", bound="EngineRatingReasonType8")


@_attrs_define
class EngineRatingReasonType8:
    """The host has no ROCm runtime (`libamdhip64`), which the Linux ROCm
    build loads from the machine.

        Attributes:
            code (EngineRatingReasonType8Code):
    """

    code: EngineRatingReasonType8Code
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
        code = EngineRatingReasonType8Code(d.pop("code"))

        engine_rating_reason_type_8 = cls(
            code=code,
        )

        engine_rating_reason_type_8.additional_properties = d
        return engine_rating_reason_type_8

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
