from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.engine_channel import EngineChannel
from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.dictation_engine import DictationEngine
    from ..models.engine_entry import EngineEntry
    from ..models.variant_update import VariantUpdate


T = TypeVar("T", bound="EnginesResponse")


@_attrs_define
class EnginesResponse:
    """Body of `GET /api/v1/engines`.

    Attributes:
        channel (EngineChannel): An update channel.
        cuda_proposable (bool): True when an NVIDIA card is detected and no CUDA engine is installed.
        engines (list[EngineEntry]): Engines embedded in the bundle, then installed ones.
        updates (list[VariantUpdate]): Per variant offered by that manifest for this platform, whether a newer
            build than the installed one exists. Empty until a manifest verified.
        dictation (DictationEngine | None | Unset):
        manifest_sequence (int | None | Unset): Sequence of the last verified manifest of that channel, if any.
        nvidia_card (None | str | Unset): Name of the NVIDIA card detected, when there is one.
    """

    channel: EngineChannel
    cuda_proposable: bool
    engines: list[EngineEntry]
    updates: list[VariantUpdate]
    dictation: DictationEngine | None | Unset = UNSET
    manifest_sequence: int | None | Unset = UNSET
    nvidia_card: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.dictation_engine import DictationEngine

        channel = self.channel.value

        cuda_proposable = self.cuda_proposable

        engines = []
        for engines_item_data in self.engines:
            engines_item = engines_item_data.to_dict()
            engines.append(engines_item)

        updates = []
        for updates_item_data in self.updates:
            updates_item = updates_item_data.to_dict()
            updates.append(updates_item)

        dictation: dict[str, Any] | None | Unset
        if isinstance(self.dictation, Unset):
            dictation = UNSET
        elif isinstance(self.dictation, DictationEngine):
            dictation = self.dictation.to_dict()
        else:
            dictation = self.dictation

        manifest_sequence: int | None | Unset
        if isinstance(self.manifest_sequence, Unset):
            manifest_sequence = UNSET
        else:
            manifest_sequence = self.manifest_sequence

        nvidia_card: None | str | Unset
        if isinstance(self.nvidia_card, Unset):
            nvidia_card = UNSET
        else:
            nvidia_card = self.nvidia_card

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "channel": channel,
                "cuda_proposable": cuda_proposable,
                "engines": engines,
                "updates": updates,
            }
        )
        if dictation is not UNSET:
            field_dict["dictation"] = dictation
        if manifest_sequence is not UNSET:
            field_dict["manifest_sequence"] = manifest_sequence
        if nvidia_card is not UNSET:
            field_dict["nvidia_card"] = nvidia_card

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.dictation_engine import DictationEngine
        from ..models.engine_entry import EngineEntry
        from ..models.variant_update import VariantUpdate

        d = dict(src_dict)
        channel = EngineChannel(d.pop("channel"))

        cuda_proposable = d.pop("cuda_proposable")

        engines = []
        _engines = d.pop("engines")
        for engines_item_data in _engines:
            engines_item = EngineEntry.from_dict(engines_item_data)

            engines.append(engines_item)

        updates = []
        _updates = d.pop("updates")
        for updates_item_data in _updates:
            updates_item = VariantUpdate.from_dict(updates_item_data)

            updates.append(updates_item)

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

        def _parse_manifest_sequence(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        manifest_sequence = _parse_manifest_sequence(d.pop("manifest_sequence", UNSET))

        def _parse_nvidia_card(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        nvidia_card = _parse_nvidia_card(d.pop("nvidia_card", UNSET))

        engines_response = cls(
            channel=channel,
            cuda_proposable=cuda_proposable,
            engines=engines,
            updates=updates,
            dictation=dictation,
            manifest_sequence=manifest_sequence,
            nvidia_card=nvidia_card,
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
