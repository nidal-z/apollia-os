from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.model_source_dto import ModelSourceDto
from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.create_preset_request_load import CreatePresetRequestLoad
    from ..models.create_preset_request_request import CreatePresetRequestRequest


T = TypeVar("T", bound="CreatePresetRequest")


@_attrs_define
class CreatePresetRequest:
    """Body of `POST /api/v1/llm/presets`.

    Attributes:
        model_id (str): The model the preset is attached to. Fixed once created.
        model_source (ModelSourceDto): Where a model runs.
        name (str): Name, unique among the presets of the model.
        is_model_default (bool | Unset): Make it the default preset of its model (default: `false`).
        load (CreatePresetRequestLoad | Unset): Load settings, written only while models.preset_load_settings is on.
            Never a system prompt.
        request (CreatePresetRequestRequest | Unset): Request settings. Unknown fields, a system prompt among them, are
            refused.
    """

    model_id: str
    model_source: ModelSourceDto
    name: str
    is_model_default: bool | Unset = UNSET
    load: CreatePresetRequestLoad | Unset = UNSET
    request: CreatePresetRequestRequest | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        model_id = self.model_id

        model_source = self.model_source.value

        name = self.name

        is_model_default = self.is_model_default

        load: dict[str, Any] | Unset = UNSET
        if not isinstance(self.load, Unset):
            load = self.load.to_dict()

        request: dict[str, Any] | Unset = UNSET
        if not isinstance(self.request, Unset):
            request = self.request.to_dict()

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "model_id": model_id,
                "model_source": model_source,
                "name": name,
            }
        )
        if is_model_default is not UNSET:
            field_dict["is_model_default"] = is_model_default
        if load is not UNSET:
            field_dict["load"] = load
        if request is not UNSET:
            field_dict["request"] = request

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.create_preset_request_load import CreatePresetRequestLoad
        from ..models.create_preset_request_request import CreatePresetRequestRequest

        d = dict(src_dict)
        model_id = d.pop("model_id")

        model_source = ModelSourceDto(d.pop("model_source"))

        name = d.pop("name")

        is_model_default = d.pop("is_model_default", UNSET)

        _load = d.pop("load", UNSET)
        load: CreatePresetRequestLoad | Unset
        if isinstance(_load, Unset):
            load = UNSET
        else:
            load = CreatePresetRequestLoad.from_dict(_load)

        _request = d.pop("request", UNSET)
        request: CreatePresetRequestRequest | Unset
        if isinstance(_request, Unset):
            request = UNSET
        else:
            request = CreatePresetRequestRequest.from_dict(_request)

        create_preset_request = cls(
            model_id=model_id,
            model_source=model_source,
            name=name,
            is_model_default=is_model_default,
            load=load,
            request=request,
        )

        create_preset_request.additional_properties = d
        return create_preset_request

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
