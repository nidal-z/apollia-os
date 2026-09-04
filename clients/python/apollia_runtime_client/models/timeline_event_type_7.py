from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..models.timeline_event_type_7_type import TimelineEventType7Type
from ..types import UNSET, Unset
from typing import cast






T = TypeVar("T", bound="TimelineEventType7")



@_attrs_define
class TimelineEventType7:
    """ Task completion (terminal event).

        Attributes:
            timestamp (str): ISO 8601 timestamp.
            type_ (TimelineEventType7Type):
            duration_ms (int | None | Unset): Total duration in milliseconds.
            output_preview (None | str | Unset): Output preview (truncated to 500 chars).
     """

    timestamp: str
    type_: TimelineEventType7Type
    duration_ms: int | None | Unset = UNSET
    output_preview: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        timestamp = self.timestamp

        type_ = self.type_.value

        duration_ms: int | None | Unset
        if isinstance(self.duration_ms, Unset):
            duration_ms = UNSET
        else:
            duration_ms = self.duration_ms

        output_preview: None | str | Unset
        if isinstance(self.output_preview, Unset):
            output_preview = UNSET
        else:
            output_preview = self.output_preview


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "timestamp": timestamp,
            "type": type_,
        })
        if duration_ms is not UNSET:
            field_dict["duration_ms"] = duration_ms
        if output_preview is not UNSET:
            field_dict["output_preview"] = output_preview

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        timestamp = d.pop("timestamp")

        type_ = TimelineEventType7Type(d.pop("type"))




        def _parse_duration_ms(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        duration_ms = _parse_duration_ms(d.pop("duration_ms", UNSET))


        def _parse_output_preview(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        output_preview = _parse_output_preview(d.pop("output_preview", UNSET))


        timeline_event_type_7 = cls(
            timestamp=timestamp,
            type_=type_,
            duration_ms=duration_ms,
            output_preview=output_preview,
        )


        timeline_event_type_7.additional_properties = d
        return timeline_event_type_7

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
