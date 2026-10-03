from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.fit_dto import FitDto

T = TypeVar("T", bound="MemoryEstimateDto")


@_attrs_define
class MemoryEstimateDto:
    """The memory a load would take.

    Attributes:
        capacity_gb (float): What the machine can hold, in gigabytes.
        fit (FitDto): How a model fits the machine.
        gpu_gb (float): On the accelerator, in gigabytes.
        kv_cache_measured (bool): Whether the cache was sized from the model's header rather than by a
            proportional fallback.
        total_gb (float): Weights, cache and engine overhead, in gigabytes.
    """

    capacity_gb: float
    fit: FitDto
    gpu_gb: float
    kv_cache_measured: bool
    total_gb: float
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        capacity_gb = self.capacity_gb

        fit = self.fit.value

        gpu_gb = self.gpu_gb

        kv_cache_measured = self.kv_cache_measured

        total_gb = self.total_gb

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "capacityGb": capacity_gb,
                "fit": fit,
                "gpuGb": gpu_gb,
                "kvCacheMeasured": kv_cache_measured,
                "totalGb": total_gb,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        capacity_gb = d.pop("capacityGb")

        fit = FitDto(d.pop("fit"))

        gpu_gb = d.pop("gpuGb")

        kv_cache_measured = d.pop("kvCacheMeasured")

        total_gb = d.pop("totalGb")

        memory_estimate_dto = cls(
            capacity_gb=capacity_gb,
            fit=fit,
            gpu_gb=gpu_gb,
            kv_cache_measured=kv_cache_measured,
            total_gb=total_gb,
        )

        memory_estimate_dto.additional_properties = d
        return memory_estimate_dto

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
