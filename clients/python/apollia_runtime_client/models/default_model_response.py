from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.model_source_dto import ModelSourceDto

T = TypeVar("T", bound="DefaultModelResponse")


@_attrs_define
class DefaultModelResponse:
    """The default model of the machine.

    Attributes:
        backend (str): Name of the backend that holds the default, the name the router routes by.
        model_id (str): The model, in the form `PUT` accepts.
        model_source (ModelSourceDto): Where a model runs.
    """

    backend: str
    model_id: str
    model_source: ModelSourceDto
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        backend = self.backend

        model_id = self.model_id

        model_source = self.model_source.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "backend": backend,
                "model_id": model_id,
                "model_source": model_source,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        backend = d.pop("backend")

        model_id = d.pop("model_id")

        model_source = ModelSourceDto(d.pop("model_source"))

        default_model_response = cls(
            backend=backend,
            model_id=model_id,
            model_source=model_source,
        )

        default_model_response.additional_properties = d
        return default_model_response

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
