from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.engine_rating_reason_type_1_code import EngineRatingReasonType1Code

T = TypeVar("T", bound="EngineRatingReasonType1")


@_attrs_define
class EngineRatingReasonType1:
    """CUDA 13 dropped the architecture of the card (Maxwell, Pascal, Volta).

    Attributes:
        cc (str): The compute capability of the card, `6.1`.
        code (EngineRatingReasonType1Code):
    """

    cc: str
    code: EngineRatingReasonType1Code
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        cc = self.cc

        code = self.code.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "cc": cc,
                "code": code,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        cc = d.pop("cc")

        code = EngineRatingReasonType1Code(d.pop("code"))

        engine_rating_reason_type_1 = cls(
            cc=cc,
            code=code,
        )

        engine_rating_reason_type_1.additional_properties = d
        return engine_rating_reason_type_1

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
