from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.model_source_dto import ModelSourceDto

if TYPE_CHECKING:
    from ..models.preset_response_load import PresetResponseLoad
    from ..models.preset_response_request import PresetResponseRequest


T = TypeVar("T", bound="PresetResponse")


@_attrs_define
class PresetResponse:
    """One preset.

    Attributes:
        id (int): Identifier.
        is_model_default (bool): Whether this is the default preset of its model.
        load (PresetResponseLoad): Load settings, applied over the automatic computation.
        model_id (str): The model: a local registry identifier, or `<server>/<model>`.
        model_source (ModelSourceDto): Where a model runs.
        name (str): Name, unique among the presets of the model.
        request (PresetResponseRequest): Request settings: `temperature`, `max_tokens`, `stop`, `top_k`,
            `top_p`, `min_p`, `repeat_penalty`, `presence_penalty`, `json_schema`.
    """

    id: int
    is_model_default: bool
    load: PresetResponseLoad
    model_id: str
    model_source: ModelSourceDto
    name: str
    request: PresetResponseRequest
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        id = self.id

        is_model_default = self.is_model_default

        load = self.load.to_dict()

        model_id = self.model_id

        model_source = self.model_source.value

        name = self.name

        request = self.request.to_dict()

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "id": id,
                "is_model_default": is_model_default,
                "load": load,
                "model_id": model_id,
                "model_source": model_source,
                "name": name,
                "request": request,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.preset_response_load import PresetResponseLoad
        from ..models.preset_response_request import PresetResponseRequest

        d = dict(src_dict)
        id = d.pop("id")

        is_model_default = d.pop("is_model_default")

        load = PresetResponseLoad.from_dict(d.pop("load"))

        model_id = d.pop("model_id")

        model_source = ModelSourceDto(d.pop("model_source"))

        name = d.pop("name")

        request = PresetResponseRequest.from_dict(d.pop("request"))

        preset_response = cls(
            id=id,
            is_model_default=is_model_default,
            load=load,
            model_id=model_id,
            model_source=model_source,
            name=name,
            request=request,
        )

        preset_response.additional_properties = d
        return preset_response

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
