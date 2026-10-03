from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="GgufModelInfo")


@_attrs_define
class GgufModelInfo:
    """GGUF model file detected on the local filesystem.

    Attributes:
        filename (str): Filename of the model.
        path (str): Absolute path to the model file.
        recommended (bool): Whether the memory estimator says this model fits this machine.
        size_bytes (int): File size in bytes.
        size_human (str): Human-readable file size (e.g. `"4.2 GB"`).
    """

    filename: str
    path: str
    recommended: bool
    size_bytes: int
    size_human: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        filename = self.filename

        path = self.path

        recommended = self.recommended

        size_bytes = self.size_bytes

        size_human = self.size_human

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "filename": filename,
                "path": path,
                "recommended": recommended,
                "size_bytes": size_bytes,
                "size_human": size_human,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        filename = d.pop("filename")

        path = d.pop("path")

        recommended = d.pop("recommended")

        size_bytes = d.pop("size_bytes")

        size_human = d.pop("size_human")

        gguf_model_info = cls(
            filename=filename,
            path=path,
            recommended=recommended,
            size_bytes=size_bytes,
            size_human=size_human,
        )

        gguf_model_info.additional_properties = d
        return gguf_model_info

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
