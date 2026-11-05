from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.gpu_card_vendor import GpuCardVendor
from ..types import UNSET, Unset

T = TypeVar("T", bound="HardwareCard")


@_attrs_define
class HardwareCard:
    """One graphics card of the machine.

    Attributes:
        name (str): Its name.
        timed_out (bool): True when the driver did not answer within the probe limit.
        vendor (GpuCardVendor): The maker of a card.
        compute_capability (None | str | Unset): The NVIDIA compute capability, `8.9`.
        driver_version (None | str | Unset): The NVIDIA driver version, `580.65.06`.
        gfx_target (None | str | Unset): The gfx target of an AMD card, `gfx1100`.
        kfd_accessible (bool | None | Unset): Whether `/dev/kfd` opens. AMD under Linux only.
        vram_gb (float | None | Unset): Its dedicated memory in GiB, when known.
    """

    name: str
    timed_out: bool
    vendor: GpuCardVendor
    compute_capability: None | str | Unset = UNSET
    driver_version: None | str | Unset = UNSET
    gfx_target: None | str | Unset = UNSET
    kfd_accessible: bool | None | Unset = UNSET
    vram_gb: float | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        name = self.name

        timed_out = self.timed_out

        vendor = self.vendor.value

        compute_capability: None | str | Unset
        if isinstance(self.compute_capability, Unset):
            compute_capability = UNSET
        else:
            compute_capability = self.compute_capability

        driver_version: None | str | Unset
        if isinstance(self.driver_version, Unset):
            driver_version = UNSET
        else:
            driver_version = self.driver_version

        gfx_target: None | str | Unset
        if isinstance(self.gfx_target, Unset):
            gfx_target = UNSET
        else:
            gfx_target = self.gfx_target

        kfd_accessible: bool | None | Unset
        if isinstance(self.kfd_accessible, Unset):
            kfd_accessible = UNSET
        else:
            kfd_accessible = self.kfd_accessible

        vram_gb: float | None | Unset
        if isinstance(self.vram_gb, Unset):
            vram_gb = UNSET
        else:
            vram_gb = self.vram_gb

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "name": name,
                "timed_out": timed_out,
                "vendor": vendor,
            }
        )
        if compute_capability is not UNSET:
            field_dict["compute_capability"] = compute_capability
        if driver_version is not UNSET:
            field_dict["driver_version"] = driver_version
        if gfx_target is not UNSET:
            field_dict["gfx_target"] = gfx_target
        if kfd_accessible is not UNSET:
            field_dict["kfd_accessible"] = kfd_accessible
        if vram_gb is not UNSET:
            field_dict["vram_gb"] = vram_gb

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        name = d.pop("name")

        timed_out = d.pop("timed_out")

        vendor = GpuCardVendor(d.pop("vendor"))

        def _parse_compute_capability(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        compute_capability = _parse_compute_capability(d.pop("compute_capability", UNSET))

        def _parse_driver_version(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        driver_version = _parse_driver_version(d.pop("driver_version", UNSET))

        def _parse_gfx_target(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        gfx_target = _parse_gfx_target(d.pop("gfx_target", UNSET))

        def _parse_kfd_accessible(data: object) -> bool | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(bool | None | Unset, data)

        kfd_accessible = _parse_kfd_accessible(d.pop("kfd_accessible", UNSET))

        def _parse_vram_gb(data: object) -> float | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(float | None | Unset, data)

        vram_gb = _parse_vram_gb(d.pop("vram_gb", UNSET))

        hardware_card = cls(
            name=name,
            timed_out=timed_out,
            vendor=vendor,
            compute_capability=compute_capability,
            driver_version=driver_version,
            gfx_target=gfx_target,
            kfd_accessible=kfd_accessible,
            vram_gb=vram_gb,
        )

        hardware_card.additional_properties = d
        return hardware_card

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
