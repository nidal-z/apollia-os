from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.resource_usage_response_vram import ResourceUsageResponseVram


T = TypeVar("T", bound="ResourceUsageResponse")


@_attrs_define
class ResourceUsageResponse:
    """The memory and the processor in use at the time of the request.

    Attributes:
        cpu_percent (float): Processor load over every core, from 0 to 100.
        ram_total_gb (float): System memory installed, in GB.
        ram_used_gb (float): System memory in use, in GB.
        vram (ResourceUsageResponseVram): Video memory in use: `{"kind":"dedicated","used_gb":..,"total_gb":..}`
            on an NVIDIA card, `{"kind":"unified"}` where the graphics processor
            shares the system memory, `{"kind":"unmeasured"}` anywhere else.
    """

    cpu_percent: float
    ram_total_gb: float
    ram_used_gb: float
    vram: ResourceUsageResponseVram
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        cpu_percent = self.cpu_percent

        ram_total_gb = self.ram_total_gb

        ram_used_gb = self.ram_used_gb

        vram = self.vram.to_dict()

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "cpu_percent": cpu_percent,
                "ram_total_gb": ram_total_gb,
                "ram_used_gb": ram_used_gb,
                "vram": vram,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.resource_usage_response_vram import ResourceUsageResponseVram

        d = dict(src_dict)
        cpu_percent = d.pop("cpu_percent")

        ram_total_gb = d.pop("ram_total_gb")

        ram_used_gb = d.pop("ram_used_gb")

        vram = ResourceUsageResponseVram.from_dict(d.pop("vram"))

        resource_usage_response = cls(
            cpu_percent=cpu_percent,
            ram_total_gb=ram_total_gb,
            ram_used_gb=ram_used_gb,
            vram=vram,
        )

        resource_usage_response.additional_properties = d
        return resource_usage_response

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
