from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.update_preset_request_load import UpdatePresetRequestLoad
    from ..models.update_preset_request_request import UpdatePresetRequestRequest


T = TypeVar("T", bound="UpdatePresetRequest")


@_attrs_define
class UpdatePresetRequest:
    """Body of `PUT /api/v1/llm/presets/{id}`.

    Attributes:
        name (str): Name, unique among the presets of the model.
        is_model_default (bool | Unset): Make it the default preset of its model (default: `false`).
        load (UpdatePresetRequestLoad | Unset): Load settings. Never a system prompt.
        request (UpdatePresetRequestRequest | Unset): Request settings. Unknown fields, a system prompt among them, are
            refused.
    """

    name: str
    is_model_default: bool | Unset = UNSET
    load: UpdatePresetRequestLoad | Unset = UNSET
    request: UpdatePresetRequestRequest | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
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
        from ..models.update_preset_request_load import UpdatePresetRequestLoad
        from ..models.update_preset_request_request import UpdatePresetRequestRequest

        d = dict(src_dict)
        name = d.pop("name")

        is_model_default = d.pop("is_model_default", UNSET)

        _load = d.pop("load", UNSET)
        load: UpdatePresetRequestLoad | Unset
        if isinstance(_load, Unset):
            load = UNSET
        else:
            load = UpdatePresetRequestLoad.from_dict(_load)

        _request = d.pop("request", UNSET)
        request: UpdatePresetRequestRequest | Unset
        if isinstance(_request, Unset):
            request = UNSET
        else:
            request = UpdatePresetRequestRequest.from_dict(_request)

        update_preset_request = cls(
            name=name,
            is_model_default=is_model_default,
            load=load,
            request=request,
        )

        update_preset_request.additional_properties = d
        return update_preset_request

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
