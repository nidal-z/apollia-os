from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.engine_channel import EngineChannel

T = TypeVar("T", bound="EngineSettingsBody")


@_attrs_define
class EngineSettingsBody:
    """Body of the settings routes.

    Attributes:
        base_url (str): Base address manifests and archives are fetched from.
        channel (EngineChannel): An update channel.
    """

    base_url: str
    channel: EngineChannel
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        base_url = self.base_url

        channel = self.channel.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "base_url": base_url,
                "channel": channel,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        base_url = d.pop("base_url")

        channel = EngineChannel(d.pop("channel"))

        engine_settings_body = cls(
            base_url=base_url,
            channel=channel,
        )

        engine_settings_body.additional_properties = d
        return engine_settings_body

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
