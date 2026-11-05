from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.engine_channel import EngineChannel
from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.available_package import AvailablePackage
    from ..models.dictation_engine import DictationEngine
    from ..models.engine_entry import EngineEntry
    from ..models.engine_rating import EngineRating
    from ..models.hardware_view import HardwareView
    from ..models.variant_update import VariantUpdate


T = TypeVar("T", bound="EnginesResponse")


@_attrs_define
class EnginesResponse:
    """Body of `GET /api/v1/engines`.

    Attributes:
        available (list[AvailablePackage]): The packages the verified manifest offers for this platform, installed
            or not. Empty until a manifest verified.
        channel (EngineChannel): An update channel.
        engines (list[EngineEntry]): Engines embedded in the bundle, then installed ones.
        ratings (list[EngineRating]): Per `llama-server` variant, whether it suits this machine and why.
            Empty until a manifest verified, or when the hardware is not known.
        updates (list[VariantUpdate]): Per variant offered by that manifest for this platform, whether a newer
            build than the installed one exists. Empty until a manifest verified.
        verifiable (bool): Whether this build carries the distribution key the channel is
            verified with. Without it no engine can be checked or installed from
            the channel, and the engines embedded in the bundle are the only ones.
        dictation (DictationEngine | None | Unset):
        hardware (HardwareView | None | Unset):
        manifest_sequence (int | None | Unset): Sequence of the last verified manifest of that channel, if any.
    """

    available: list[AvailablePackage]
    channel: EngineChannel
    engines: list[EngineEntry]
    ratings: list[EngineRating]
    updates: list[VariantUpdate]
    verifiable: bool
    dictation: DictationEngine | None | Unset = UNSET
    hardware: HardwareView | None | Unset = UNSET
    manifest_sequence: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.dictation_engine import DictationEngine
        from ..models.hardware_view import HardwareView

        available = []
        for available_item_data in self.available:
            available_item = available_item_data.to_dict()
            available.append(available_item)

        channel = self.channel.value

        engines = []
        for engines_item_data in self.engines:
            engines_item = engines_item_data.to_dict()
            engines.append(engines_item)

        ratings = []
        for ratings_item_data in self.ratings:
            ratings_item = ratings_item_data.to_dict()
            ratings.append(ratings_item)

        updates = []
        for updates_item_data in self.updates:
            updates_item = updates_item_data.to_dict()
            updates.append(updates_item)

        verifiable = self.verifiable

        dictation: dict[str, Any] | None | Unset
        if isinstance(self.dictation, Unset):
            dictation = UNSET
        elif isinstance(self.dictation, DictationEngine):
            dictation = self.dictation.to_dict()
        else:
            dictation = self.dictation

        hardware: dict[str, Any] | None | Unset
        if isinstance(self.hardware, Unset):
            hardware = UNSET
        elif isinstance(self.hardware, HardwareView):
            hardware = self.hardware.to_dict()
        else:
            hardware = self.hardware

        manifest_sequence: int | None | Unset
        if isinstance(self.manifest_sequence, Unset):
            manifest_sequence = UNSET
        else:
            manifest_sequence = self.manifest_sequence

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "available": available,
                "channel": channel,
                "engines": engines,
                "ratings": ratings,
                "updates": updates,
                "verifiable": verifiable,
            }
        )
        if dictation is not UNSET:
            field_dict["dictation"] = dictation
        if hardware is not UNSET:
            field_dict["hardware"] = hardware
        if manifest_sequence is not UNSET:
            field_dict["manifest_sequence"] = manifest_sequence

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.available_package import AvailablePackage
        from ..models.dictation_engine import DictationEngine
        from ..models.engine_entry import EngineEntry
        from ..models.engine_rating import EngineRating
        from ..models.hardware_view import HardwareView
        from ..models.variant_update import VariantUpdate

        d = dict(src_dict)
        available = []
        _available = d.pop("available")
        for available_item_data in _available:
            available_item = AvailablePackage.from_dict(available_item_data)

            available.append(available_item)

        channel = EngineChannel(d.pop("channel"))

        engines = []
        _engines = d.pop("engines")
        for engines_item_data in _engines:
            engines_item = EngineEntry.from_dict(engines_item_data)

            engines.append(engines_item)

        ratings = []
        _ratings = d.pop("ratings")
        for ratings_item_data in _ratings:
            ratings_item = EngineRating.from_dict(ratings_item_data)

            ratings.append(ratings_item)

        updates = []
        _updates = d.pop("updates")
        for updates_item_data in _updates:
            updates_item = VariantUpdate.from_dict(updates_item_data)

            updates.append(updates_item)

        verifiable = d.pop("verifiable")

        def _parse_dictation(data: object) -> DictationEngine | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                dictation_type_1 = DictationEngine.from_dict(data)

                return dictation_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(DictationEngine | None | Unset, data)

        dictation = _parse_dictation(d.pop("dictation", UNSET))

        def _parse_hardware(data: object) -> HardwareView | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                hardware_type_1 = HardwareView.from_dict(data)

                return hardware_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(HardwareView | None | Unset, data)

        hardware = _parse_hardware(d.pop("hardware", UNSET))

        def _parse_manifest_sequence(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        manifest_sequence = _parse_manifest_sequence(d.pop("manifest_sequence", UNSET))

        engines_response = cls(
            available=available,
            channel=channel,
            engines=engines,
            ratings=ratings,
            updates=updates,
            verifiable=verifiable,
            dictation=dictation,
            hardware=hardware,
            manifest_sequence=manifest_sequence,
        )

        engines_response.additional_properties = d
        return engines_response

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
