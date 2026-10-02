from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.model_source_dto import ModelSourceDto
from ..types import UNSET, Unset

T = TypeVar("T", bound="SetAgentModelRequest")


@_attrs_define
class SetAgentModelRequest:
    """Body of `PUT /api/v1/llm/agents/{agent}/model`.

    Attributes:
        model_id (str): The model: a local registry identifier, or `<server>/<model>` of a
            declared server.
        model_source (ModelSourceDto): Where a model runs.
        preset_id (int | None | Unset): A preset of that same model, or `null` for the model's default.
    """

    model_id: str
    model_source: ModelSourceDto
    preset_id: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        model_id = self.model_id

        model_source = self.model_source.value

        preset_id: int | None | Unset
        if isinstance(self.preset_id, Unset):
            preset_id = UNSET
        else:
            preset_id = self.preset_id

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "model_id": model_id,
                "model_source": model_source,
            }
        )
        if preset_id is not UNSET:
            field_dict["preset_id"] = preset_id

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        model_id = d.pop("model_id")

        model_source = ModelSourceDto(d.pop("model_source"))

        def _parse_preset_id(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        preset_id = _parse_preset_id(d.pop("preset_id", UNSET))

        set_agent_model_request = cls(
            model_id=model_id,
            model_source=model_source,
            preset_id=preset_id,
        )

        set_agent_model_request.additional_properties = d
        return set_agent_model_request

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
