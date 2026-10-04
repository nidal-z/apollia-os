from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="ModelPin")


@_attrs_define
class ModelPin:
    """Request and response body for `PUT /api/v1/llm/pins`.

    Attributes:
        model_path (str): Path of the model, as the resident list reports it.
        pinned (bool): `true` keeps it loaded until it is unloaded by hand.
    """

    model_path: str
    pinned: bool
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        model_path = self.model_path

        pinned = self.pinned

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "modelPath": model_path,
                "pinned": pinned,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        model_path = d.pop("modelPath")

        pinned = d.pop("pinned")

        model_pin = cls(
            model_path=model_path,
            pinned=pinned,
        )

        model_pin.additional_properties = d
        return model_pin

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
