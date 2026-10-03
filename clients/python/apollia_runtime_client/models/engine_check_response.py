from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.engine_channel import EngineChannel

if TYPE_CHECKING:
    from ..models.variant_update import VariantUpdate


T = TypeVar("T", bound="EngineCheckResponse")


@_attrs_define
class EngineCheckResponse:
    """Body of `POST /api/v1/engines/check`.

    Attributes:
        channel (EngineChannel): An update channel.
        manifest_sequence (int): Sequence of the manifest just verified.
        updates (list[VariantUpdate]): Per variant, whether an update exists.
    """

    channel: EngineChannel
    manifest_sequence: int
    updates: list[VariantUpdate]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        channel = self.channel.value

        manifest_sequence = self.manifest_sequence

        updates = []
        for updates_item_data in self.updates:
            updates_item = updates_item_data.to_dict()
            updates.append(updates_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "channel": channel,
                "manifest_sequence": manifest_sequence,
                "updates": updates,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.variant_update import VariantUpdate

        d = dict(src_dict)
        channel = EngineChannel(d.pop("channel"))

        manifest_sequence = d.pop("manifest_sequence")

        updates = []
        _updates = d.pop("updates")
        for updates_item_data in _updates:
            updates_item = VariantUpdate.from_dict(updates_item_data)

            updates.append(updates_item)

        engine_check_response = cls(
            channel=channel,
            manifest_sequence=manifest_sequence,
            updates=updates,
        )

        engine_check_response.additional_properties = d
        return engine_check_response

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
