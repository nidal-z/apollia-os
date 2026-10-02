from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="UnloadModelResponse")


@_attrs_define
class UnloadModelResponse:
    """Response body for `POST /api/v1/llm/loaded-models/unload`.

    Attributes:
        model_path (str): The model that was unloaded.
        stopped (int): Processes stopped: one, or more when the model ran with several
            context windows.
    """

    model_path: str
    stopped: int
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        model_path = self.model_path

        stopped = self.stopped

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "modelPath": model_path,
                "stopped": stopped,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        model_path = d.pop("modelPath")

        stopped = d.pop("stopped")

        unload_model_response = cls(
            model_path=model_path,
            stopped=stopped,
        )

        unload_model_response.additional_properties = d
        return unload_model_response

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
