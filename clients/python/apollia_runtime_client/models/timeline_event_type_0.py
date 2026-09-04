from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..models.timeline_event_type_0_type import TimelineEventType0Type






T = TypeVar("T", bound="TimelineEventType0")



@_attrs_define
class TimelineEventType0:
    """ Task state transition (submitted -> running -> completed, etc.).

        Attributes:
            status (str): Target status name.
            timestamp (str): ISO 8601 timestamp of the transition.
            type_ (TimelineEventType0Type):
     """

    status: str
    timestamp: str
    type_: TimelineEventType0Type
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        status = self.status

        timestamp = self.timestamp

        type_ = self.type_.value


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "status": status,
            "timestamp": timestamp,
            "type": type_,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        status = d.pop("status")

        timestamp = d.pop("timestamp")

        type_ = TimelineEventType0Type(d.pop("type"))




        timeline_event_type_0 = cls(
            status=status,
            timestamp=timestamp,
            type_=type_,
        )


        timeline_event_type_0.additional_properties = d
        return timeline_event_type_0

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
