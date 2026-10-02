from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.loaded_model_dto import LoadedModelDto


T = TypeVar("T", bound="LoadedModelsResponse")


@_attrs_define
class LoadedModelsResponse:
    """Response body for `GET /api/v1/llm/loaded-models`.

    Attributes:
        engine_available (bool): Whether a local engine exists on this host. `false` means nothing can
            ever be loaded, not merely that nothing is.
        max_loaded (int): How many models may stay resident at once; `0` without an engine.
        models (list[LoadedModelDto]): The resident models, most recently used first.
    """

    engine_available: bool
    max_loaded: int
    models: list[LoadedModelDto]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        engine_available = self.engine_available

        max_loaded = self.max_loaded

        models = []
        for models_item_data in self.models:
            models_item = models_item_data.to_dict()
            models.append(models_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "engineAvailable": engine_available,
                "maxLoaded": max_loaded,
                "models": models,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.loaded_model_dto import LoadedModelDto

        d = dict(src_dict)
        engine_available = d.pop("engineAvailable")

        max_loaded = d.pop("maxLoaded")

        models = []
        _models = d.pop("models")
        for models_item_data in _models:
            models_item = LoadedModelDto.from_dict(models_item_data)

            models.append(models_item)

        loaded_models_response = cls(
            engine_available=engine_available,
            max_loaded=max_loaded,
            models=models,
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
