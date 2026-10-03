from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="WhisperModelInfo")


@_attrs_define
class WhisperModelInfo:
    """Whisper GGML model file detected on the local filesystem.

    Attributes:
        filename (str): Filename of the model.
        model_size (str): Size variant parsed from the filename (`tiny`, `base`, `small`, `medium`, `large`).
        path (str): Absolute path to the model file.
        recommended (bool): Whether the dictation verdict says this file fits this machine.
        size_bytes (int): File size in bytes.
    """

    filename: str
    model_size: str
    path: str
    recommended: bool
    size_bytes: int
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        filename = self.filename

        model_size = self.model_size

        path = self.path

        recommended = self.recommended

        size_bytes = self.size_bytes

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "filename": filename,
                "model_size": model_size,
                "path": path,
                "recommended": recommended,
                "size_bytes": size_bytes,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        filename = d.pop("filename")

        model_size = d.pop("model_size")

        path = d.pop("path")

        recommended = d.pop("recommended")

        size_bytes = d.pop("size_bytes")

        whisper_model_info = cls(
            filename=filename,
            model_size=model_size,
            path=path,
            recommended=recommended,
            size_bytes=size_bytes,
        )

        whisper_model_info.additional_properties = d
        return whisper_model_info

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
