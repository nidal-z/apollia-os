from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..models.timeline_event_type_2_type import TimelineEventType2Type
from ..types import UNSET, Unset
from typing import cast






T = TypeVar("T", bound="TimelineEventType2")



@_attrs_define
class TimelineEventType2:
    """ Completion of an ORIA step.

        Attributes:
            step_id (str): Step identifier.
            success (bool): `true` if the step finished successfully.
            timestamp (str): ISO 8601 timestamp.
            type_ (TimelineEventType2Type):
            duration_ms (int | None | Unset): Execution duration in milliseconds.
     """

    step_id: str
    success: bool
    timestamp: str
    type_: TimelineEventType2Type
    duration_ms: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        step_id = self.step_id

        success = self.success

        timestamp = self.timestamp

        type_ = self.type_.value

        duration_ms: int | None | Unset
        if isinstance(self.duration_ms, Unset):
            duration_ms = UNSET
        else:
            duration_ms = self.duration_ms


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "step_id": step_id,
            "success": success,
            "timestamp": timestamp,
            "type": type_,
        })
        if duration_ms is not UNSET:
            field_dict["duration_ms"] = duration_ms

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        step_id = d.pop("step_id")

        success = d.pop("success")

        timestamp = d.pop("timestamp")

        type_ = TimelineEventType2Type(d.pop("type"))




        def _parse_duration_ms(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        duration_ms = _parse_duration_ms(d.pop("duration_ms", UNSET))


        timeline_event_type_2 = cls(
            step_id=step_id,
            success=success,
            timestamp=timestamp,
            type_=type_,
            duration_ms=duration_ms,
        )


        timeline_event_type_2.additional_properties = d
        return timeline_event_type_2

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
