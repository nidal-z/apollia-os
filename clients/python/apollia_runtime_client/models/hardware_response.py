from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from typing import cast

if TYPE_CHECKING:
  from ..models.hardware_response_accelerator import HardwareResponseAccelerator





T = TypeVar("T", bound="HardwareResponse")



@_attrs_define
class HardwareResponse:
    """
        Attributes:
            accelerator (HardwareResponseAccelerator):
            available_ram_gb (float):
            cpu_cores (int):
            cpu_model (str):
            memory_budget_gb (float):
            total_ram_gb (float):
     """

    accelerator: HardwareResponseAccelerator
    available_ram_gb: float
    cpu_cores: int
    cpu_model: str
    memory_budget_gb: float
    total_ram_gb: float
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.hardware_response_accelerator import HardwareResponseAccelerator
        accelerator = self.accelerator.to_dict()

        available_ram_gb = self.available_ram_gb

        cpu_cores = self.cpu_cores

        cpu_model = self.cpu_model

        memory_budget_gb = self.memory_budget_gb

        total_ram_gb = self.total_ram_gb


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "accelerator": accelerator,
            "available_ram_gb": available_ram_gb,
            "cpu_cores": cpu_cores,
            "cpu_model": cpu_model,
            "memory_budget_gb": memory_budget_gb,
            "total_ram_gb": total_ram_gb,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.hardware_response_accelerator import HardwareResponseAccelerator
        d = dict(src_dict)
        accelerator = HardwareResponseAccelerator.from_dict(d.pop("accelerator"))




        available_ram_gb = d.pop("available_ram_gb")

        cpu_cores = d.pop("cpu_cores")

        cpu_model = d.pop("cpu_model")

        memory_budget_gb = d.pop("memory_budget_gb")

        total_ram_gb = d.pop("total_ram_gb")

        hardware_response = cls(
            accelerator=accelerator,
            available_ram_gb=available_ram_gb,
            cpu_cores=cpu_cores,
            cpu_model=cpu_model,
            memory_budget_gb=memory_budget_gb,
            total_ram_gb=total_ram_gb,
        )


        hardware_response.additional_properties = d
        return hardware_response

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
