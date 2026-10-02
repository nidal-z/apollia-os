from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.counted_language_model import CountedLanguageModel
    from ..models.dictation_models_response_models_item import DictationModelsResponseModelsItem
    from ..models.dictation_models_response_reason_type_0 import DictationModelsResponseReasonType0
    from ..models.hardware_response import HardwareResponse


T = TypeVar("T", bound="DictationModelsResponse")


@_attrs_define
class DictationModelsResponse:
    """The dictation category of the registry, and the entry for this machine.

    Attributes:
        hardware (HardwareResponse):
        models (list[DictationModelsResponseModelsItem]): Every entry of the category, with its footprint and whether it
            fits.
        language_model (CountedLanguageModel | None | Unset):
        reason (DictationModelsResponseReasonType0 | None | Unset): Why: `fits_beside` or `no_room_beside`, with the
            footprints compared,
            in gibibytes.
        recommended (None | str | Unset): Identifier of the entry recommended for this machine.
    """

    hardware: HardwareResponse
    models: list[DictationModelsResponseModelsItem]
    language_model: CountedLanguageModel | None | Unset = UNSET
    reason: DictationModelsResponseReasonType0 | None | Unset = UNSET
    recommended: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.counted_language_model import CountedLanguageModel
        from ..models.dictation_models_response_reason_type_0 import (
            DictationModelsResponseReasonType0,
        )

        hardware = self.hardware.to_dict()

        models = []
        for models_item_data in self.models:
            models_item = models_item_data.to_dict()
            models.append(models_item)

        language_model: dict[str, Any] | None | Unset
        if isinstance(self.language_model, Unset):
            language_model = UNSET
        elif isinstance(self.language_model, CountedLanguageModel):
            language_model = self.language_model.to_dict()
        else:
            language_model = self.language_model

        reason: dict[str, Any] | None | Unset
        if isinstance(self.reason, Unset):
            reason = UNSET
        elif isinstance(self.reason, DictationModelsResponseReasonType0):
            reason = self.reason.to_dict()
        else:
            reason = self.reason

        recommended: None | str | Unset
        if isinstance(self.recommended, Unset):
            recommended = UNSET
        else:
            recommended = self.recommended

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "hardware": hardware,
                "models": models,
            }
        )
        if language_model is not UNSET:
            field_dict["language_model"] = language_model
        if reason is not UNSET:
            field_dict["reason"] = reason
        if recommended is not UNSET:
            field_dict["recommended"] = recommended

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.counted_language_model import CountedLanguageModel
        from ..models.dictation_models_response_models_item import DictationModelsResponseModelsItem
        from ..models.dictation_models_response_reason_type_0 import (
            DictationModelsResponseReasonType0,
        )
        from ..models.hardware_response import HardwareResponse

        d = dict(src_dict)
        hardware = HardwareResponse.from_dict(d.pop("hardware"))

        models = []
        _models = d.pop("models")
        for models_item_data in _models:
            models_item = DictationModelsResponseModelsItem.from_dict(models_item_data)

            models.append(models_item)

        def _parse_language_model(data: object) -> CountedLanguageModel | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                language_model_type_1 = CountedLanguageModel.from_dict(data)

                return language_model_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(CountedLanguageModel | None | Unset, data)

        language_model = _parse_language_model(d.pop("language_model", UNSET))

        def _parse_reason(data: object) -> DictationModelsResponseReasonType0 | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                reason_type_0 = DictationModelsResponseReasonType0.from_dict(data)

                return reason_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(DictationModelsResponseReasonType0 | None | Unset, data)

        reason = _parse_reason(d.pop("reason", UNSET))

        def _parse_recommended(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        recommended = _parse_recommended(d.pop("recommended", UNSET))

        dictation_models_response = cls(
            hardware=hardware,
            models=models,
            language_model=language_model,
            reason=reason,
            recommended=recommended,
        )

        dictation_models_response.additional_properties = d
        return dictation_models_response

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
