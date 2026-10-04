from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.loaded_model_dto import LoadedModelDto


T = TypeVar("T", bound="LoadModelResponse")


@_attrs_define
class LoadModelResponse:
    """Response of a load by hand.

    Attributes:
        loaded (LoadedModelDto): One resident model.
        model_id (str): The model, as asked.
        remembered_preset_id (int | None | Unset): The default preset the settings were written into, when `remember`
            was asked.
    """

    loaded: LoadedModelDto
    model_id: str
    remembered_preset_id: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        loaded = self.loaded.to_dict()

        model_id = self.model_id

        remembered_preset_id: int | None | Unset
        if isinstance(self.remembered_preset_id, Unset):
            remembered_preset_id = UNSET
        else:
            remembered_preset_id = self.remembered_preset_id

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "loaded": loaded,
                "modelId": model_id,
            }
        )
        if remembered_preset_id is not UNSET:
            field_dict["rememberedPresetId"] = remembered_preset_id

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.loaded_model_dto import LoadedModelDto

        d = dict(src_dict)
        loaded = LoadedModelDto.from_dict(d.pop("loaded"))

        model_id = d.pop("modelId")

        def _parse_remembered_preset_id(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        remembered_preset_id = _parse_remembered_preset_id(d.pop("rememberedPresetId", UNSET))

        load_model_response = cls(
            loaded=loaded,
            model_id=model_id,
            remembered_preset_id=remembered_preset_id,
        )

        load_model_response.additional_properties = d
        return load_model_response

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
