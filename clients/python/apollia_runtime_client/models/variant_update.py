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
        update_available (bool): True when a tag is installed and the manifest offers a newer one.
        variant (EngineVariant): A build of an engine for one kind of hardware.
        installed_tag (None | str | Unset): Newest tag installed here, if any.
    """

    engine: EngineName
    latest_tag: str
    update_available: bool
    variant: EngineVariant
    installed_tag: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        engine = self.engine.value

        latest_tag = self.latest_tag

        update_available = self.update_available

        variant = self.variant.value

        installed_tag: None | str | Unset
        if isinstance(self.installed_tag, Unset):
            installed_tag = UNSET
        else:
            installed_tag = self.installed_tag

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "engine": engine,
                "latest_tag": latest_tag,
                "update_available": update_available,
                "variant": variant,
            }
        )
        if installed_tag is not UNSET:
            field_dict["installed_tag"] = installed_tag

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        engine = EngineName(d.pop("engine"))

        latest_tag = d.pop("latest_tag")

        update_available = d.pop("update_available")

        variant = EngineVariant(d.pop("variant"))

        def _parse_installed_tag(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        installed_tag = _parse_installed_tag(d.pop("installed_tag", UNSET))

        variant_update = cls(
            engine=engine,
            latest_tag=latest_tag,
            update_available=update_available,
            variant=variant,
            installed_tag=installed_tag,
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
