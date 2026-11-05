from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.engine_rating_reason_type_0_code import EngineRatingReasonType0Code

T = TypeVar("T", bound="EngineRatingReasonType0")


@_attrs_define
class EngineRatingReasonType0:
    """The NVIDIA driver is older than the build needs.

    Attributes:
        code (EngineRatingReasonType0Code):
        have (str): The driver of the machine, as `nvidia-smi` prints it.
        need (str): The driver the build needs.
    """

    code: EngineRatingReasonType0Code
    have: str
    need: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        code = self.code.value

        have = self.have

        need = self.need

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "code": code,
                "have": have,
                "need": need,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        code = EngineRatingReasonType0Code(d.pop("code"))

        have = d.pop("have")

        need = d.pop("need")

        engine_rating_reason_type_0 = cls(
            code=code,
            have=have,
            need=need,
        )

        engine_rating_reason_type_0.additional_properties = d
        return engine_rating_reason_type_0

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
