from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.fail_closed_type_1_cause import FailClosedType1Cause

T = TypeVar("T", bound="FailClosedType1")


@_attrs_define
class FailClosedType1:
    """An organisation policy is deposited and refused.

    Attributes:
        cause (FailClosedType1Cause):
        detail (str): What exactly was refused.
        reason (str): The reason class: `schema`, `signature`, `key` or `owner`.
    """

    cause: FailClosedType1Cause
    detail: str
    reason: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        cause = self.cause.value

        detail = self.detail

        reason = self.reason

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "cause": cause,
                "detail": detail,
                "reason": reason,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        cause = FailClosedType1Cause(d.pop("cause"))

        detail = d.pop("detail")

        reason = d.pop("reason")

        fail_closed_type_1 = cls(
            cause=cause,
            detail=detail,
            reason=reason,
        )

        fail_closed_type_1.additional_properties = d
        return fail_closed_type_1

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
