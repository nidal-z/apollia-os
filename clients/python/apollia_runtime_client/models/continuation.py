from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.continuation_refusal import ContinuationRefusal
from ..types import UNSET, Unset

T = TypeVar("T", bound="Continuation")


@_attrs_define
class Continuation:
    """Whether the last answer of a conversation can be continued.

    Attributes:
        available (bool): `true` when the model of the conversation continues an answer.
        reason (ContinuationRefusal | None | Unset):
    """

    available: bool
    reason: ContinuationRefusal | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        available = self.available

        reason: None | str | Unset
        if isinstance(self.reason, Unset):
            reason = UNSET
        elif isinstance(self.reason, ContinuationRefusal):
            reason = self.reason.value
        else:
            reason = self.reason

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "available": available,
            }
        )
        if reason is not UNSET:
            field_dict["reason"] = reason

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        available = d.pop("available")

        def _parse_reason(data: object) -> ContinuationRefusal | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, str):
                    raise TypeError()
                reason_type_1 = ContinuationRefusal(data)

                return reason_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(ContinuationRefusal | None | Unset, data)

        reason = _parse_reason(d.pop("reason", UNSET))

        continuation = cls(
            available=available,
            reason=reason,
        )

        continuation.additional_properties = d
        return continuation

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
