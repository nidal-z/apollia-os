from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.global_timeline_event_detail import GlobalTimelineEventDetail


T = TypeVar("T", bound="GlobalTimelineEvent")


@_attrs_define
class GlobalTimelineEvent:
    """Global timeline event for display.

    Attributes:
        detail (GlobalTimelineEventDetail): Expandable JSON details.
        event_type (str): Event type: task, tool, llm, trigger, hitl, memory, error.
        summary (str): Event summary.
        timestamp (str): ISO 8601 timestamp.
    """

    detail: GlobalTimelineEventDetail
    event_type: str
    summary: str
    timestamp: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        detail = self.detail.to_dict()

        event_type = self.event_type

        summary = self.summary

        timestamp = self.timestamp

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "detail": detail,
                "event_type": event_type,
                "summary": summary,
                "timestamp": timestamp,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.global_timeline_event_detail import GlobalTimelineEventDetail

        d = dict(src_dict)
        detail = GlobalTimelineEventDetail.from_dict(d.pop("detail"))

        event_type = d.pop("event_type")

        summary = d.pop("summary")

        timestamp = d.pop("timestamp")

        global_timeline_event = cls(
            detail=detail,
            event_type=event_type,
            summary=summary,
            timestamp=timestamp,
        )

        global_timeline_event.additional_properties = d
        return global_timeline_event

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
