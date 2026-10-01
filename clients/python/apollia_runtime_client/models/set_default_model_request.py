from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.model_source_dto import ModelSourceDto

T = TypeVar("T", bound="SetDefaultModelRequest")


@_attrs_define
class SetDefaultModelRequest:
    """Body of `PUT /api/v1/llm/default-model`.

    Attributes:
        model_id (str): `local`: the name of a local engine backend. `server`: `<server>/<model>`
            of a declared server, or the name of a server backend.
        model_source (ModelSourceDto): Where a model runs.
    """

    model_id: str
    model_source: ModelSourceDto
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        model_id = self.model_id

        model_source = self.model_source.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "model_id": model_id,
                "model_source": model_source,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        model_id = d.pop("model_id")

        model_source = ModelSourceDto(d.pop("model_source"))

        set_default_model_request = cls(
            model_id=model_id,
            model_source=model_source,
        )

        set_default_model_request.additional_properties = d
        return set_default_model_request

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
