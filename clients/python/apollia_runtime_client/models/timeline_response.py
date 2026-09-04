from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from typing import cast

if TYPE_CHECKING:
  from ..models.timeline_event_type_0 import TimelineEventType0
  from ..models.timeline_event_type_1 import TimelineEventType1
  from ..models.timeline_event_type_2 import TimelineEventType2
  from ..models.timeline_event_type_3 import TimelineEventType3
  from ..models.timeline_event_type_4 import TimelineEventType4
  from ..models.timeline_event_type_5 import TimelineEventType5
  from ..models.timeline_event_type_6 import TimelineEventType6
  from ..models.timeline_event_type_7 import TimelineEventType7





T = TypeVar("T", bound="TimelineResponse")



@_attrs_define
class TimelineResponse:
    """ Timeline response.

        Attributes:
            events (list[TimelineEventType0 | TimelineEventType1 | TimelineEventType2 | TimelineEventType3 |
                TimelineEventType4 | TimelineEventType5 | TimelineEventType6 | TimelineEventType7]): Events sorted by timestamp
                ascending.
            task_id (str): Task identifier.
     """

    events: list[TimelineEventType0 | TimelineEventType1 | TimelineEventType2 | TimelineEventType3 | TimelineEventType4 | TimelineEventType5 | TimelineEventType6 | TimelineEventType7]
    task_id: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.timeline_event_type_0 import TimelineEventType0
        from ..models.timeline_event_type_1 import TimelineEventType1
        from ..models.timeline_event_type_2 import TimelineEventType2
        from ..models.timeline_event_type_3 import TimelineEventType3
        from ..models.timeline_event_type_4 import TimelineEventType4
        from ..models.timeline_event_type_5 import TimelineEventType5
        from ..models.timeline_event_type_6 import TimelineEventType6
        from ..models.timeline_event_type_7 import TimelineEventType7
        events = []
        for events_item_data in self.events:
            events_item: dict[str, Any]
            if isinstance(events_item_data, TimelineEventType0):
                events_item = events_item_data.to_dict()
            elif isinstance(events_item_data, TimelineEventType1):
                events_item = events_item_data.to_dict()
            elif isinstance(events_item_data, TimelineEventType2):
                events_item = events_item_data.to_dict()
            elif isinstance(events_item_data, TimelineEventType3):
                events_item = events_item_data.to_dict()
            elif isinstance(events_item_data, TimelineEventType4):
                events_item = events_item_data.to_dict()
            elif isinstance(events_item_data, TimelineEventType5):
                events_item = events_item_data.to_dict()
            elif isinstance(events_item_data, TimelineEventType6):
                events_item = events_item_data.to_dict()
            else:
                events_item = events_item_data.to_dict()

            events.append(events_item)



        task_id = self.task_id


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "events": events,
            "task_id": task_id,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.timeline_event_type_0 import TimelineEventType0
        from ..models.timeline_event_type_1 import TimelineEventType1
        from ..models.timeline_event_type_2 import TimelineEventType2
        from ..models.timeline_event_type_3 import TimelineEventType3
        from ..models.timeline_event_type_4 import TimelineEventType4
        from ..models.timeline_event_type_5 import TimelineEventType5
        from ..models.timeline_event_type_6 import TimelineEventType6
        from ..models.timeline_event_type_7 import TimelineEventType7
        d = dict(src_dict)
        events = []
        _events = d.pop("events")
        for events_item_data in (_events):
            def _parse_events_item(data: object) -> TimelineEventType0 | TimelineEventType1 | TimelineEventType2 | TimelineEventType3 | TimelineEventType4 | TimelineEventType5 | TimelineEventType6 | TimelineEventType7:
                try:
                    if not isinstance(data, dict):
                        raise TypeError()
                    componentsschemas_timeline_event_type_0 = TimelineEventType0.from_dict(data)



                    return componentsschemas_timeline_event_type_0
                except (TypeError, ValueError, AttributeError, KeyError):
                    pass
                try:
                    if not isinstance(data, dict):
                        raise TypeError()
                    componentsschemas_timeline_event_type_1 = TimelineEventType1.from_dict(data)



                    return componentsschemas_timeline_event_type_1
                except (TypeError, ValueError, AttributeError, KeyError):
                    pass
                try:
                    if not isinstance(data, dict):
                        raise TypeError()
                    componentsschemas_timeline_event_type_2 = TimelineEventType2.from_dict(data)



                    return componentsschemas_timeline_event_type_2
                except (TypeError, ValueError, AttributeError, KeyError):
                    pass
                try:
                    if not isinstance(data, dict):
                        raise TypeError()
                    componentsschemas_timeline_event_type_3 = TimelineEventType3.from_dict(data)



                    return componentsschemas_timeline_event_type_3
                except (TypeError, ValueError, AttributeError, KeyError):
                    pass
                try:
                    if not isinstance(data, dict):
                        raise TypeError()
                    componentsschemas_timeline_event_type_4 = TimelineEventType4.from_dict(data)



                    return componentsschemas_timeline_event_type_4
                except (TypeError, ValueError, AttributeError, KeyError):
                    pass
                try:
                    if not isinstance(data, dict):
                        raise TypeError()
                    componentsschemas_timeline_event_type_5 = TimelineEventType5.from_dict(data)



                    return componentsschemas_timeline_event_type_5
                except (TypeError, ValueError, AttributeError, KeyError):
                    pass
                try:
                    if not isinstance(data, dict):
                        raise TypeError()
                    componentsschemas_timeline_event_type_6 = TimelineEventType6.from_dict(data)



                    return componentsschemas_timeline_event_type_6
                except (TypeError, ValueError, AttributeError, KeyError):
                    pass
                if not isinstance(data, dict):
                    raise TypeError()
                componentsschemas_timeline_event_type_7 = TimelineEventType7.from_dict(data)



                return componentsschemas_timeline_event_type_7

            events_item = _parse_events_item(events_item_data)

            events.append(events_item)


        task_id = d.pop("task_id")

        timeline_response = cls(
            events=events,
            task_id=task_id,
        )


        timeline_response.additional_properties = d
        return timeline_response

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
