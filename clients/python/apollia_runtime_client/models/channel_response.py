from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast

if TYPE_CHECKING:
  from ..models.channel_response_config import ChannelResponseConfig





T = TypeVar("T", bound="ChannelResponse")



@_attrs_define
class ChannelResponse:
    """ Full notification channel returned by the CRUD operations.

        Attributes:
            channel_type (str): Channel type.
            config (ChannelResponseConfig): Type-specific configuration.
            created_at (str): Creation timestamp (ISO 8601).
            enabled (bool): `true` if the channel is enabled.
            id (str): Unique channel identifier.
            min_interval_seconds (int): Minimum throttling interval, in seconds.
            updated_at (str): Last modification timestamp (ISO 8601).
            events (list[str] | None | Unset): Channel-specific events.
            label (None | str | Unset): Free-form display name. `null` falls back to `id` in the UI.
     """

    channel_type: str
    config: ChannelResponseConfig
    created_at: str
    enabled: bool
    id: str
    min_interval_seconds: int
    updated_at: str
    events: list[str] | None | Unset = UNSET
    label: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.channel_response_config import ChannelResponseConfig
        channel_type = self.channel_type

        config = self.config.to_dict()

        created_at = self.created_at

        enabled = self.enabled

        id = self.id

        min_interval_seconds = self.min_interval_seconds

        updated_at = self.updated_at

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


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "channel_type": channel_type,
            "config": config,
            "created_at": created_at,
            "enabled": enabled,
            "id": id,
            "min_interval_seconds": min_interval_seconds,
            "updated_at": updated_at,
        })
        if events is not UNSET:
            field_dict["events"] = events
        if label is not UNSET:
            field_dict["label"] = label

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.channel_response_config import ChannelResponseConfig
        d = dict(src_dict)
        channel_type = d.pop("channel_type")

        config = ChannelResponseConfig.from_dict(d.pop("config"))




        created_at = d.pop("created_at")

        enabled = d.pop("enabled")

        id = d.pop("id")

        min_interval_seconds = d.pop("min_interval_seconds")

        updated_at = d.pop("updated_at")

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


        channel_response = cls(
            channel_type=channel_type,
            config=config,
            created_at=created_at,
            enabled=enabled,
            id=id,
            min_interval_seconds=min_interval_seconds,
            updated_at=updated_at,
            events=events,
            label=label,
        )


        channel_response.additional_properties = d
        return channel_response

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
