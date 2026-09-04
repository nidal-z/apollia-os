from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast

if TYPE_CHECKING:
  from ..models.trace_response_events_item import TraceResponseEventsItem





T = TypeVar("T", bound="TraceResponse")



@_attrs_define
class TraceResponse:
    """ JSON response.

        Attributes:
            events (list[TraceResponseEventsItem]): Events ordered chronologically (UUIDv7 ASC).
            task_id (str): Task identifier.
            next_cursor (None | str | Unset): Cursor to pass as `?since=` on the next call to fetch the rest.
                `None` when the current page reaches the known end.
     """

    events: list[TraceResponseEventsItem]
    task_id: str
    next_cursor: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.trace_response_events_item import TraceResponseEventsItem
        events = []
        for events_item_data in self.events:
            events_item = events_item_data.to_dict()
            events.append(events_item)



        task_id = self.task_id

        next_cursor: None | str | Unset
        if isinstance(self.next_cursor, Unset):
            next_cursor = UNSET
        else:
            next_cursor = self.next_cursor


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "events": events,
            "task_id": task_id,
        })
        if next_cursor is not UNSET:
            field_dict["next_cursor"] = next_cursor

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.trace_response_events_item import TraceResponseEventsItem
        d = dict(src_dict)
        events = []
        _events = d.pop("events")
        for events_item_data in (_events):
            events_item = TraceResponseEventsItem.from_dict(events_item_data)



            events.append(events_item)


        task_id = d.pop("task_id")

        def _parse_next_cursor(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        next_cursor = _parse_next_cursor(d.pop("next_cursor", UNSET))


        trace_response = cls(
            events=events,
            task_id=task_id,
            next_cursor=next_cursor,
        )


        trace_response.additional_properties = d
        return trace_response

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
