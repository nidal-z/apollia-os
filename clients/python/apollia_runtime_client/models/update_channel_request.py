from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast

if TYPE_CHECKING:
  from ..models.update_channel_request_config_type_0 import UpdateChannelRequestConfigType0





T = TypeVar("T", bound="UpdateChannelRequest")



@_attrs_define
class UpdateChannelRequest:
    """ Request body for `PUT /api/v1/notifications/channels/:id`.

    The `label` field uses a double `Option`:
    - absent from JSON: `None`, keep the existing label;
    - `null`: `Some(None)`, clear the label;
    - `"text"`: `Some(Some("text"))`, replace it.

        Attributes:
            channel_type (None | str | Unset): Channel type (optional, keeps the existing one if absent).
            config (None | Unset | UpdateChannelRequestConfigType0): Type-specific configuration.
            enabled (bool | None | Unset): Whether the channel is active.
            events (list[str] | None | Unset): Channel-specific event list.
            label (None | str | Unset): New label. See the struct docs for the double-Option semantics.
            min_interval_seconds (int | None | Unset): New minimum throttling interval (s). Absent keeps the existing one.
     """

    channel_type: None | str | Unset = UNSET
    config: None | Unset | UpdateChannelRequestConfigType0 = UNSET
    enabled: bool | None | Unset = UNSET
    events: list[str] | None | Unset = UNSET
    label: None | str | Unset = UNSET
    min_interval_seconds: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.update_channel_request_config_type_0 import UpdateChannelRequestConfigType0
        channel_type: None | str | Unset
        if isinstance(self.channel_type, Unset):
            channel_type = UNSET
        else:
            channel_type = self.channel_type

        config: dict[str, Any] | None | Unset
        if isinstance(self.config, Unset):
            config = UNSET
        elif isinstance(self.config, UpdateChannelRequestConfigType0):
            config = self.config.to_dict()
        else:
            config = self.config

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

        min_interval_seconds: int | None | Unset
        if isinstance(self.min_interval_seconds, Unset):
            min_interval_seconds = UNSET
        else:
            min_interval_seconds = self.min_interval_seconds


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
        })
        if channel_type is not UNSET:
            field_dict["channel_type"] = channel_type
        if config is not UNSET:
            field_dict["config"] = config
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
        from ..models.update_channel_request_config_type_0 import UpdateChannelRequestConfigType0
        d = dict(src_dict)
        def _parse_channel_type(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        channel_type = _parse_channel_type(d.pop("channel_type", UNSET))


        def _parse_config(data: object) -> None | Unset | UpdateChannelRequestConfigType0:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                config_type_0 = UpdateChannelRequestConfigType0.from_dict(data)



                return config_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | Unset | UpdateChannelRequestConfigType0, data)

        config = _parse_config(d.pop("config", UNSET))


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


        def _parse_min_interval_seconds(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        min_interval_seconds = _parse_min_interval_seconds(d.pop("min_interval_seconds", UNSET))


        update_channel_request = cls(
            channel_type=channel_type,
            config=config,
            enabled=enabled,
            events=events,
            label=label,
            min_interval_seconds=min_interval_seconds,
        )


        update_channel_request.additional_properties = d
        return update_channel_request

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
