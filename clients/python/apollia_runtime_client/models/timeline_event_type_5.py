from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..models.timeline_event_type_5_type import TimelineEventType5Type






T = TypeVar("T", bound="TimelineEventType5")



@_attrs_define
class TimelineEventType5:
    """ HITL suspension: the agent requests human approval.

        Attributes:
            prompt (str): Prompt shown to the operator.
            timestamp (str): ISO 8601 timestamp of the suspension.
            type_ (TimelineEventType5Type):
     """

    prompt: str
    timestamp: str
    type_: TimelineEventType5Type
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        prompt = self.prompt

        timestamp = self.timestamp

        type_ = self.type_.value


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "prompt": prompt,
            "timestamp": timestamp,
            "type": type_,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        prompt = d.pop("prompt")

        timestamp = d.pop("timestamp")

        type_ = TimelineEventType5Type(d.pop("type"))




        timeline_event_type_5 = cls(
            prompt=prompt,
            timestamp=timestamp,
            type_=type_,
        )


        timeline_event_type_5.additional_properties = d
        return timeline_event_type_5

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
