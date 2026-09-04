from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..models.timeline_event_type_6_type import TimelineEventType6Type
from ..types import UNSET, Unset
from typing import cast






T = TypeVar("T", bound="TimelineEventType6")



@_attrs_define
class TimelineEventType6:
    """ HITL resolution: the operator has responded.

        Attributes:
            approved (bool): `true` if approved, `false` if rejected.
            timestamp (str): ISO 8601 timestamp of the response.
            type_ (TimelineEventType6Type):
            reason (None | str | Unset): Reason provided by the operator.
            wait_ms (int | None | Unset): Wait duration in milliseconds.
     """

    approved: bool
    timestamp: str
    type_: TimelineEventType6Type
    reason: None | str | Unset = UNSET
    wait_ms: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        approved = self.approved

        timestamp = self.timestamp

        type_ = self.type_.value

        reason: None | str | Unset
        if isinstance(self.reason, Unset):
            reason = UNSET
        else:
            reason = self.reason

        wait_ms: int | None | Unset
        if isinstance(self.wait_ms, Unset):
            wait_ms = UNSET
        else:
            wait_ms = self.wait_ms


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "approved": approved,
            "timestamp": timestamp,
            "type": type_,
        })
        if reason is not UNSET:
            field_dict["reason"] = reason
        if wait_ms is not UNSET:
            field_dict["wait_ms"] = wait_ms

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        approved = d.pop("approved")

        timestamp = d.pop("timestamp")

        type_ = TimelineEventType6Type(d.pop("type"))




        def _parse_reason(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        reason = _parse_reason(d.pop("reason", UNSET))


        def _parse_wait_ms(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        wait_ms = _parse_wait_ms(d.pop("wait_ms", UNSET))


        timeline_event_type_6 = cls(
            approved=approved,
            timestamp=timestamp,
            type_=type_,
            reason=reason,
            wait_ms=wait_ms,
        )


        timeline_event_type_6.additional_properties = d
        return timeline_event_type_6

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
