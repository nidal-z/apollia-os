from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..models.timeline_event_type_4_type import TimelineEventType4Type
from ..types import UNSET, Unset
from typing import cast






T = TypeVar("T", bound="TimelineEventType4")



@_attrs_define
class TimelineEventType4:
    """ Invocation of a native tool.

        Attributes:
            timestamp (str): ISO 8601 timestamp.
            tool_name (str): Tool name.
            truncated (bool): `true` if the data was truncated.
            type_ (TimelineEventType4Type):
            duration_ms (int | None | Unset): Duration in milliseconds.
            exit_code (int | None | Unset): Process exit code (bash, python).
            input_preview (None | str | Unset): Input preview (args_json), truncated to 300 chars.
            output_preview (None | str | Unset): Output preview (stdout + stderr), truncated to 500 chars.
     """

    timestamp: str
    tool_name: str
    truncated: bool
    type_: TimelineEventType4Type
    duration_ms: int | None | Unset = UNSET
    exit_code: int | None | Unset = UNSET
    input_preview: None | str | Unset = UNSET
    output_preview: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        timestamp = self.timestamp

        tool_name = self.tool_name

        truncated = self.truncated

        type_ = self.type_.value

        duration_ms: int | None | Unset
        if isinstance(self.duration_ms, Unset):
            duration_ms = UNSET
        else:
            duration_ms = self.duration_ms

        exit_code: int | None | Unset
        if isinstance(self.exit_code, Unset):
            exit_code = UNSET
        else:
            exit_code = self.exit_code

        input_preview: None | str | Unset
        if isinstance(self.input_preview, Unset):
            input_preview = UNSET
        else:
            input_preview = self.input_preview

        output_preview: None | str | Unset
        if isinstance(self.output_preview, Unset):
            output_preview = UNSET
        else:
            output_preview = self.output_preview


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "timestamp": timestamp,
            "tool_name": tool_name,
            "truncated": truncated,
            "type": type_,
        })
        if duration_ms is not UNSET:
            field_dict["duration_ms"] = duration_ms
        if exit_code is not UNSET:
            field_dict["exit_code"] = exit_code
        if input_preview is not UNSET:
            field_dict["input_preview"] = input_preview
        if output_preview is not UNSET:
            field_dict["output_preview"] = output_preview

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        timestamp = d.pop("timestamp")

        tool_name = d.pop("tool_name")

        truncated = d.pop("truncated")

        type_ = TimelineEventType4Type(d.pop("type"))




        def _parse_duration_ms(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        duration_ms = _parse_duration_ms(d.pop("duration_ms", UNSET))


        def _parse_exit_code(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        exit_code = _parse_exit_code(d.pop("exit_code", UNSET))


        def _parse_input_preview(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        input_preview = _parse_input_preview(d.pop("input_preview", UNSET))


        def _parse_output_preview(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        output_preview = _parse_output_preview(d.pop("output_preview", UNSET))


        timeline_event_type_4 = cls(
            timestamp=timestamp,
            tool_name=tool_name,
            truncated=truncated,
            type_=type_,
            duration_ms=duration_ms,
            exit_code=exit_code,
            input_preview=input_preview,
            output_preview=output_preview,
        )


        timeline_event_type_4.additional_properties = d
        return timeline_event_type_4

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
