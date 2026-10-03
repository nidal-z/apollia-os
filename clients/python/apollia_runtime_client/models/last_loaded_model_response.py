from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.last_loaded_model_response_load import LastLoadedModelResponseLoad


T = TypeVar("T", bound="LastLoadedModelResponse")


@_attrs_define
class LastLoadedModelResponse:
    """Response of `GET /api/v1/llm/loaded-models/last`.

    Attributes:
        load (LastLoadedModelResponseLoad): The settings it was loaded with, in the keys a preset's load settings
            use.
        loaded_at (str): When it was loaded, RFC 3339.
        model_id (str): The model, by the identifier the model index gives it.
    """

    load: LastLoadedModelResponseLoad
    loaded_at: str
    model_id: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        load = self.load.to_dict()

        loaded_at = self.loaded_at

        model_id = self.model_id

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "load": load,
                "loadedAt": loaded_at,
                "modelId": model_id,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.last_loaded_model_response_load import LastLoadedModelResponseLoad

        d = dict(src_dict)
        load = LastLoadedModelResponseLoad.from_dict(d.pop("load"))

        loaded_at = d.pop("loadedAt")

        model_id = d.pop("modelId")

        last_loaded_model_response = cls(
            load=load,
            loaded_at=loaded_at,
            model_id=model_id,
        )

        last_loaded_model_response.additional_properties = d
        return last_loaded_model_response

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
