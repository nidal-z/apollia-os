from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast

if TYPE_CHECKING:
  from ..models.create_channel_request_config import CreateChannelRequestConfig





T = TypeVar("T", bound="CreateChannelRequest")



@_attrs_define
class CreateChannelRequest:
    """ Request body for `POST /api/v1/notifications/channels`.

        Attributes:
            channel_type (str): Channel type: `"desktop"` or `"webhook"`.
            config (CreateChannelRequestConfig): Type-specific configuration (e.g. `{"url": "..."}` for webhook).
            id (str): Unique channel identifier.
            enabled (bool | None | Unset): Whether the channel is active (default: `true`).
            events (list[str] | None | Unset): Channel-specific event list. `null` uses the global events.
            label (None | str | Unset): Free-form display name. `None` falls back to `id` in the UI.
            min_interval_seconds (int | Unset): Minimum throttling interval, in seconds. Default: `0` (none).
     """

    channel_type: str
    config: CreateChannelRequestConfig
    id: str
    enabled: bool | None | Unset = UNSET
    events: list[str] | None | Unset = UNSET
    label: None | str | Unset = UNSET
    min_interval_seconds: int | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.create_channel_request_config import CreateChannelRequestConfig
        channel_type = self.channel_type

        config = self.config.to_dict()

        id = self.id

        enabled: bool | None | Unset
        if isinstance(self.enabled, Unset):
            enabled = UNSET
        else:
            enabled = self.enabled

        events: list[str] | None | Unset
        if isinstance(self.events, Unset):
            events = UNSET
        elif isinstance(self.events, list):
            events = self.events


        else:
            events = self.events

        label: None | str | Unset
        if isinstance(self.label, Unset):
            label = UNSET
        else:
            label = self.label

        min_interval_seconds = self.min_interval_seconds


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "channel_type": channel_type,
            "config": config,
            "id": id,
        })
        if enabled is not UNSET:
            field_dict["enabled"] = enabled
        if events is not UNSET:
            field_dict["events"] = events
        if label is not UNSET:
            field_dict["label"] = label
        if min_interval_seconds is not UNSET:
            field_dict["min_interval_seconds"] = min_interval_seconds

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.create_channel_request_config import CreateChannelRequestConfig
        d = dict(src_dict)
        channel_type = d.pop("channel_type")

        config = CreateChannelRequestConfig.from_dict(d.pop("config"))




        id = d.pop("id")

        def _parse_enabled(data: object) -> bool | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(bool | None | Unset, data)

        enabled = _parse_enabled(d.pop("enabled", UNSET))


        def _parse_events(data: object) -> list[str] | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, list):
                    raise TypeError()
                events_type_0 = cast(list[str], data)

                return events_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(list[str] | None | Unset, data)

        events = _parse_events(d.pop("events", UNSET))


        def _parse_label(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        label = _parse_label(d.pop("label", UNSET))


        min_interval_seconds = d.pop("min_interval_seconds", UNSET)

        create_channel_request = cls(
            channel_type=channel_type,
            config=config,
            id=id,
            enabled=enabled,
            events=events,
            label=label,
            min_interval_seconds=min_interval_seconds,
        )


        create_channel_request.additional_properties = d
        return create_channel_request

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
