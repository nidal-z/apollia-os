from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..models.timeline_event_type_1_type import TimelineEventType1Type
from ..types import UNSET, Unset
from typing import cast






T = TypeVar("T", bound="TimelineEventType1")



@_attrs_define
class TimelineEventType1:
    """ Start of an ORIA step (orchestrated mode).

        Attributes:
            step_id (str): Step identifier.
            timestamp (str): ISO 8601 timestamp.
            type_ (TimelineEventType1Type):
            input_preview (None | str | Unset): Input preview (truncated to 200 chars).
            tool (None | str | Unset): Tool used or suggested.
     """

    step_id: str
    timestamp: str
    type_: TimelineEventType1Type
    input_preview: None | str | Unset = UNSET
    tool: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        step_id = self.step_id

        timestamp = self.timestamp

        type_ = self.type_.value

        input_preview: None | str | Unset
        if isinstance(self.input_preview, Unset):
            input_preview = UNSET
        else:
            input_preview = self.input_preview

        tool: None | str | Unset
        if isinstance(self.tool, Unset):
            tool = UNSET
        else:
            tool = self.tool


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "step_id": step_id,
            "timestamp": timestamp,
            "type": type_,
        })
        if input_preview is not UNSET:
            field_dict["input_preview"] = input_preview
        if tool is not UNSET:
            field_dict["tool"] = tool

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        step_id = d.pop("step_id")

        timestamp = d.pop("timestamp")

        type_ = TimelineEventType1Type(d.pop("type"))




        def _parse_input_preview(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        input_preview = _parse_input_preview(d.pop("input_preview", UNSET))


        def _parse_tool(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        tool = _parse_tool(d.pop("tool", UNSET))


        timeline_event_type_1 = cls(
            step_id=step_id,
            timestamp=timestamp,
            type_=type_,
            input_preview=input_preview,
            tool=tool,
        )


        timeline_event_type_1.additional_properties = d
        return timeline_event_type_1

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
