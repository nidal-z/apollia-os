from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.engine_name import EngineName
from ..models.engine_variant import EngineVariant
from ..types import UNSET, Unset

T = TypeVar("T", bound="VariantUpdate")


@_attrs_define
class VariantUpdate:
    """Whether a newer build of one engine variant exists on the channel.

    Attributes:
        engine (EngineName): An inference engine the store can hold.
        latest_tag (str): Newest tag the verified manifest offers.
        size (int): Size in bytes of the archive of the newest build, as the verified
            manifest states it: what installing or updating downloads.
        update_available (bool): True when a tag is installed and the manifest offers a newer one.
        variant (EngineVariant): A build of an engine for one kind of hardware.
        installed_released (None | str | Unset): Day the installed build was published, when its manifest stated it.
        installed_tag (None | str | Unset): Newest tag installed here, if any.
        latest_released (None | str | Unset): Day the newest build was published, `YYYY-MM-DD`, when the manifest
            states it.
    """

    engine: EngineName
    latest_tag: str
    size: int
    update_available: bool
    variant: EngineVariant
    installed_released: None | str | Unset = UNSET
    installed_tag: None | str | Unset = UNSET
    latest_released: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        engine = self.engine.value

        latest_tag = self.latest_tag

        size = self.size

        update_available = self.update_available

        variant = self.variant.value

        installed_released: None | str | Unset
        if isinstance(self.installed_released, Unset):
            installed_released = UNSET
        else:
            installed_released = self.installed_released

        installed_tag: None | str | Unset
        if isinstance(self.installed_tag, Unset):
            installed_tag = UNSET
        else:
            installed_tag = self.installed_tag

        latest_released: None | str | Unset
        if isinstance(self.latest_released, Unset):
            latest_released = UNSET
        else:
            latest_released = self.latest_released

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "engine": engine,
                "latest_tag": latest_tag,
                "size": size,
                "update_available": update_available,
                "variant": variant,
            }
        )
        if installed_released is not UNSET:
            field_dict["installed_released"] = installed_released
        if installed_tag is not UNSET:
            field_dict["installed_tag"] = installed_tag
        if latest_released is not UNSET:
            field_dict["latest_released"] = latest_released

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        engine = EngineName(d.pop("engine"))

        latest_tag = d.pop("latest_tag")

        size = d.pop("size")

        update_available = d.pop("update_available")

        variant = EngineVariant(d.pop("variant"))

        def _parse_installed_released(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        installed_released = _parse_installed_released(d.pop("installed_released", UNSET))

        def _parse_installed_tag(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        installed_tag = _parse_installed_tag(d.pop("installed_tag", UNSET))

        def _parse_latest_released(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        latest_released = _parse_latest_released(d.pop("latest_released", UNSET))

        variant_update = cls(
            engine=engine,
            latest_tag=latest_tag,
            size=size,
            update_available=update_available,
            variant=variant,
            installed_released=installed_released,
            installed_tag=installed_tag,
            latest_released=latest_released,
        )

        variant_update.additional_properties = d
        return variant_update

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
