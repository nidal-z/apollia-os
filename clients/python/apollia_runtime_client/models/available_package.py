from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.engine_name import EngineName
from ..models.engine_variant import EngineVariant
from ..types import UNSET, Unset

T = TypeVar("T", bound="AvailablePackage")


@_attrs_define
class AvailablePackage:
    """One package the channel offers for this platform.

    Attributes:
        engine (EngineName): An inference engine the store can hold.
        installed (bool): Whether this exact build is installed here.
        size (int): Archive size in bytes.
        tag (str): Upstream tag.
        variant (EngineVariant): A build of an engine for one kind of hardware.
        released (None | str | Unset): Day the build was published, when the manifest states it.
    """

    engine: EngineName
    installed: bool
    size: int
    tag: str
    variant: EngineVariant
    released: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        engine = self.engine.value

        installed = self.installed

        size = self.size

        tag = self.tag

        variant = self.variant.value

        released: None | str | Unset
        if isinstance(self.released, Unset):
            released = UNSET
        else:
            released = self.released

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "engine": engine,
                "installed": installed,
                "size": size,
                "tag": tag,
                "variant": variant,
            }
        )
        if released is not UNSET:
            field_dict["released"] = released

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        engine = EngineName(d.pop("engine"))

        installed = d.pop("installed")

        size = d.pop("size")

        tag = d.pop("tag")

        variant = EngineVariant(d.pop("variant"))

        def _parse_released(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        released = _parse_released(d.pop("released", UNSET))

        available_package = cls(
            engine=engine,
            installed=installed,
            size=size,
            tag=tag,
            variant=variant,
            released=released,
        )

        available_package.additional_properties = d
        return available_package

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
