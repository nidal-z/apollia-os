from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset







T = TypeVar("T", bound="SttStatusResponse")



@_attrs_define
class SttStatusResponse:
    """ Response body for `GET /api/v1/stt/status`.

        Attributes:
            backend_name (str): Name of the active backend (e.g. `"whisper-cpp"`).
            cuda_enabled (bool): `true` when compiled with NVIDIA CUDA GPU acceleration.
            enabled (bool): Whether STT is enabled in configuration.
            metal_enabled (bool): `true` when compiled with Apple Metal GPU acceleration.
            model_loaded (bool): Whether the model is loaded and ready for inference.
            model_name (str): Short model name (derived from filename without extension).
            model_path (str): Filesystem path of the loaded model.
     """

    backend_name: str
    cuda_enabled: bool
    enabled: bool
    metal_enabled: bool
    model_loaded: bool
    model_name: str
    model_path: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        backend_name = self.backend_name

        cuda_enabled = self.cuda_enabled

        enabled = self.enabled

        metal_enabled = self.metal_enabled

        model_loaded = self.model_loaded

        model_name = self.model_name

        model_path = self.model_path


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "backend_name": backend_name,
            "cuda_enabled": cuda_enabled,
            "enabled": enabled,
            "metal_enabled": metal_enabled,
            "model_loaded": model_loaded,
            "model_name": model_name,
            "model_path": model_path,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        backend_name = d.pop("backend_name")

        cuda_enabled = d.pop("cuda_enabled")

        enabled = d.pop("enabled")

        metal_enabled = d.pop("metal_enabled")

        model_loaded = d.pop("model_loaded")

        model_name = d.pop("model_name")

        model_path = d.pop("model_path")

        stt_status_response = cls(
            backend_name=backend_name,
            cuda_enabled=cuda_enabled,
            enabled=enabled,
            metal_enabled=metal_enabled,
            model_loaded=model_loaded,
            model_name=model_name,
            model_path=model_path,
        )


        stt_status_response.additional_properties = d
        return stt_status_response

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
