from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.engine_name import EngineName
from ..models.engine_variant import EngineVariant

T = TypeVar("T", bound="EngineInstallRequest")


@_attrs_define
class EngineInstallRequest:
    """Body of `POST /api/v1/engines/install`.

    Attributes:
        engine (EngineName): An inference engine the store can hold.
        variant (EngineVariant): A build of an engine for one kind of hardware.
    """

    engine: EngineName
    variant: EngineVariant
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        engine = self.engine.value

        variant = self.variant.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "engine": engine,
                "variant": variant,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        engine = EngineName(d.pop("engine"))

        variant = EngineVariant(d.pop("variant"))

        engine_install_request = cls(
            engine=engine,
            variant=variant,
        )

        engine_install_request.additional_properties = d
        return engine_install_request

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
