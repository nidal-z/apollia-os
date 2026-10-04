from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.loaded_model_dto import LoadedModelDto


T = TypeVar("T", bound="LoadedModelsResponse")


@_attrs_define
class LoadedModelsResponse:
    """Response body for `GET /api/v1/llm/loaded-models`.

    Attributes:
        engine_available (bool): Whether a local engine exists on this host. `false` means nothing can
            ever be loaded, not merely that nothing is.
        models (list[LoadedModelDto]): The resident models, most recently used first.
        max_loaded (int | None | Unset): The explicit ceiling on resident models (`APOLLIA_LLAMA_MAX_LOADED`);
            `null` when the load guardrail alone decides how many stay loaded, and
            without an engine.
    """

    engine_available: bool
    models: list[LoadedModelDto]
    max_loaded: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        engine_available = self.engine_available

        models = []
        for models_item_data in self.models:
            models_item = models_item_data.to_dict()
            models.append(models_item)

        max_loaded: int | None | Unset
        if isinstance(self.max_loaded, Unset):
            max_loaded = UNSET
        else:
            max_loaded = self.max_loaded

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "engineAvailable": engine_available,
                "models": models,
            }
        )
        if max_loaded is not UNSET:
            field_dict["maxLoaded"] = max_loaded

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.loaded_model_dto import LoadedModelDto

        d = dict(src_dict)
        engine_available = d.pop("engineAvailable")

        models = []
        _models = d.pop("models")
        for models_item_data in _models:
            models_item = LoadedModelDto.from_dict(models_item_data)

            models.append(models_item)

        def _parse_max_loaded(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        max_loaded = _parse_max_loaded(d.pop("maxLoaded", UNSET))

        loaded_models_response = cls(
            engine_available=engine_available,
            models=models,
            max_loaded=max_loaded,
        )

        loaded_models_response.additional_properties = d
        return loaded_models_response

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
