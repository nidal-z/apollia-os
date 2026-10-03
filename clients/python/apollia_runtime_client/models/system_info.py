from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="SystemInfo")


@_attrs_define
class SystemInfo:
    """System information used to compute model recommendations in the AI setup step.

    Attributes:
        arch (str): CPU architecture identifier (e.g. `"aarch64"`, `"x86_64"`).
        available_ram_gb (float): Available RAM in gigabytes.
        gpu_available (bool): Whether a GPU is available (basic heuristic).
        os (str): Operating system identifier (e.g. `"macos"`, `"linux"`).
        total_ram_gb (float): Total RAM in gigabytes.
        gpu_vram_gb (float | None | Unset): The accelerator's dedicated memory in gigabytes, when the engine's
            device list names one. `None` on a processor-only machine and on
            platforms where the heuristic above answers without a device list.
    """

    arch: str
    available_ram_gb: float
    gpu_available: bool
    os: str
    total_ram_gb: float
    gpu_vram_gb: float | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        arch = self.arch

        available_ram_gb = self.available_ram_gb

        gpu_available = self.gpu_available

        os = self.os

        total_ram_gb = self.total_ram_gb

        gpu_vram_gb: float | None | Unset
        if isinstance(self.gpu_vram_gb, Unset):
            gpu_vram_gb = UNSET
        else:
            gpu_vram_gb = self.gpu_vram_gb

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "arch": arch,
                "available_ram_gb": available_ram_gb,
                "gpu_available": gpu_available,
                "os": os,
                "total_ram_gb": total_ram_gb,
            }
        )
        if gpu_vram_gb is not UNSET:
            field_dict["gpu_vram_gb"] = gpu_vram_gb

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        arch = d.pop("arch")

        available_ram_gb = d.pop("available_ram_gb")

        gpu_available = d.pop("gpu_available")

        os = d.pop("os")

        total_ram_gb = d.pop("total_ram_gb")

        def _parse_gpu_vram_gb(data: object) -> float | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(float | None | Unset, data)

        gpu_vram_gb = _parse_gpu_vram_gb(d.pop("gpu_vram_gb", UNSET))

        system_info = cls(
            arch=arch,
            available_ram_gb=available_ram_gb,
            gpu_available=gpu_available,
            os=os,
            total_ram_gb=total_ram_gb,
            gpu_vram_gb=gpu_vram_gb,
        )

        system_info.additional_properties = d
        return system_info

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
